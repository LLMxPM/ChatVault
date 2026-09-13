"""只在内存中分析 WXGF 的有界 HEVC 分片，不调用微信 DLL 或输出图片。"""

import io
import re

from PIL import Image

MAX_PARTITIONS = 100
MAX_PIXELS = 40_000_000


def locate_partitions(body):
    """结合 Annex B 起始码与前置大端长度定位候选分片，不推断动画语义。"""
    if len(body) < 15 or body[:4] != b"wxgf":
        raise ValueError("invalid_wxgf_header")
    start = body[4]
    if not 11 <= start < len(body):
        raise ValueError("invalid_wxgf_header_length")
    marker = re.compile(b"\x00\x00\x00\x01|\x00\x00\x01")
    partitions = []
    while start < len(body):
        match = marker.search(body, start)
        if match is None:
            break
        offset = match.start()
        size = int.from_bytes(body[offset - 4 : offset], "big")
        if size <= match.end() - offset + 2 or offset + size > len(body):
            start = offset + 1
            continue
        partitions.append((offset, size))
        if len(partitions) > MAX_PARTITIONS:
            raise ValueError("wxgf_partition_limit")
        start = offset + size
    if not partitions:
        raise ValueError("wxgf_no_bounded_partition")
    return partitions


def decode_partition(data, pixel_budget):
    """用标准 HEVC 软件解码器读取完整分片，将像素在内存中往返 PNG。"""
    import av
    from codec import validate_image

    frames, pixels = [], 0
    with av.logging.Capture(local=True) as messages:
        with av.open(io.BytesIO(data), format="hevc", options={"err_detect": "explode"}) as container:
            container.streams.video[0].codec_context.thread_count = 1
            for frame in container.decode(video=0):
                pixels += frame.width * frame.height
                if pixels > pixel_budget or len(frames) >= 100:
                    raise ValueError("wxgf_decode_resource_limit")
                if frame.is_corrupt:
                    raise ValueError("wxgf_corrupt_frame")
                rgb = frame.reformat(format="rgb24")
                plane = rgb.planes[0]
                image = Image.frombytes("RGB", (rgb.width, rgb.height), bytes(plane), "raw", "RGB", plane.line_size)
                buffer = io.BytesIO()
                image.save(buffer, "PNG")
                details = validate_image(buffer.getvalue())
                with Image.open(io.BytesIO(buffer.getvalue())) as restored:
                    if restored.tobytes() != image.tobytes():
                        raise ValueError("wxgf_png_pixel_mismatch")
                frames.append({"width": details["width"], "height": details["height"], "source_pixel_format": frame.format.name, "png_bytes": len(buffer.getbuffer())})
        if any(level <= av.logging.ERROR for level, _, _ in messages):
            raise ValueError("wxgf_decoder_error")
    if not frames:
        raise ValueError("wxgf_no_decoded_frame")
    return frames, pixels


def inspect_payload(body):
    """报告分片可解码性；即使像素可读，也不冒充完整 WXGF 语义验证。"""
    try:
        partitions = locate_partitions(body)
        result = {
            "header_length": body[4],
            "header_version": int.from_bytes(body[5:7], "big"),
            "header_width": int.from_bytes(body[7:9], "big"),
            "header_height": int.from_bytes(body[9:11], "big"),
            "partition_count": len(partitions),
            "partition_bytes": [size for _, size in partitions],
            "bytes_after_last_partition": len(body) - sum(partitions[-1]),
        }
        decoded, remaining = [], MAX_PIXELS
        for offset, size in partitions:
            frames, pixels = decode_partition(body[offset : offset + size], remaining)
            decoded.append(frames)
            remaining -= pixels
        result["decoded_partitions"] = decoded
        header_size = (result["header_width"], result["header_height"])
        aligned_size = tuple((value + 1) // 2 * 2 for value in header_size)
        result["all_dimensions_match_even_alignment"] = all((frame["width"], frame["height"]) == aligned_size for frames in decoded for frame in frames)
        single = len(decoded) == 1 and len(decoded[0]) == 1
        result["status"] = "single_frame_png_roundtrip" if single else "multiple_frames_require_semantics"
        if single:
            frame = decoded[0][0]
            result["header_dimensions_match"] = (frame["width"], frame["height"]) == (result["header_width"], result["header_height"])
        return result
    except Exception:
        return {"status": "wxgf_analysis_incomplete"}
