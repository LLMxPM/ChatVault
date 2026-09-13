"""微信图片抽样的候选结构解密与严格解码；不写出密钥和图片内容。"""

import hashlib
import io
import re
import struct
import warnings

from Crypto.Cipher import AES
from Crypto.Util.Padding import unpad
from PIL import Image, ImageFile

MAGIC = b"\x07\x08V2\x08\x07"
MAX_PIXELS = 40_000_000
MAX_FRAMES = 100
Image.MAX_IMAGE_PIXELS = MAX_PIXELS
ImageFile.LOAD_TRUNCATED_IMAGES = False


def signature(data):
    """只识别公开的格式签名，返回格式名，绝不返回原始字节。"""
    if data.startswith(b"\xff\xd8\xff"):
        return "JPEG"
    if data.startswith(b"\x89PNG\r\n\x1a\n"):
        return "PNG"
    if data.startswith((b"GIF87a", b"GIF89a")):
        return "GIF"
    if data[:4] == b"RIFF" and data[8:12] == b"WEBP":
        return "WEBP"
    if data.startswith(b"wxgf"):
        return "WXGF"
    return None


def parse_header(data):
    """按 AES 填充段、原样中间段、文件尾 XOR 段解析并检查边界。"""
    if len(data) < 31 or data[:6] != MAGIC:
        return {"status": "unsupported_container"}
    aes_length, xor_length = struct.unpack("<II", data[6:14])
    cipher_length = (aes_length // 16 + 1) * 16
    middle_length = len(data) - 15 - cipher_length - xor_length
    valid = aes_length > 0 and data[14] == 1 and middle_length >= 0
    return {
        "status": "ready" if valid else "unsupported_structure",
        "aes_length": aes_length,
        "cipher_length": cipher_length,
        "xor_length": xor_length,
        "middle_length": middle_length,
        "flag": data[14],
    }


def account_variants(account):
    """仅评估完整目录名及公开的账号规范化规则，不枚举身份或参数。"""
    result = [("full_directory_name", account)]
    parts = account.split("_")
    if account.startswith("wxid_") and len(parts) >= 3:
        result.append(("documented_wxid_cleanup", "_".join(parts[:2])))
    elif re.fullmatch(r".+_[0-9a-fA-F]{4}", account):
        result.append(("documented_hex_suffix_cleanup", account.rsplit("_", 1)[0]))
    return result


def candidate_keys(account, codes):
    """从有限本机参数生成任务内候选；密钥使用后尽力清零可变缓冲。"""
    for rule, account_id in account_variants(account):
        for code in codes:
            seed = bytearray(f"{code}{account_id}".encode("utf-8"))
            key = bytearray(hashlib.md5(seed).hexdigest()[:16].encode("ascii"))
            seed[:] = b"\0" * len(seed)
            try:
                yield rule, key, code & 255
            finally:
                key[:] = b"\0" * len(key)


def validate_image(body):
    """检查容器尾部并验证所有帧；返回格式和尺寸，不保留像素。"""
    detected = signature(body)
    if detected not in {"JPEG", "PNG", "GIF", "WEBP"}:
        raise ValueError("unsupported_payload")
    if detected == "JPEG" and not body.endswith(b"\xff\xd9"):
        raise ValueError("invalid_image")
    if detected == "PNG" and body[-12:] != b"\0\0\0\0IEND\xaeB`\x82":
        raise ValueError("invalid_image")
    if detected == "GIF" and not body.endswith(b";"):
        raise ValueError("invalid_image")
    if detected == "WEBP" and struct.unpack("<I", body[4:8])[0] + 8 != len(body):
        raise ValueError("invalid_image")
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        with Image.open(io.BytesIO(body)) as image:
            if image.format != detected:
                raise ValueError("invalid_image")
            image.verify()
        with Image.open(io.BytesIO(body)) as image:
            frames = getattr(image, "n_frames", 1)
            width, height = image.size
            if frames > MAX_FRAMES or width * height * frames > MAX_PIXELS:
                raise ValueError("image_resource_limit")
            for frame in range(frames):
                image.seek(frame)
                image.load()
            return {"format": detected, "width": width, "height": height, "frames": frames}


def decrypt_body(data, header, cipher, xor_byte):
    """严格去除 PKCS#7 填充后拼接三段，返回调用方负责清理的缓冲。"""
    aes_end = 15 + header["cipher_length"]
    xor_start = len(data) - header["xor_length"]
    padded = bytearray(cipher.decrypt(data[15:aes_end]))
    try:
        body = bytearray(unpad(padded, 16))
    finally:
        padded[:] = b"\0" * len(padded)
    if len(body) != header["aes_length"]:
        body[:] = b"\0" * len(body)
        raise ValueError("decrypted_head_length_mismatch")
    body.extend(data[aes_end:xor_start])
    body.extend(value ^ xor_byte for value in data[xor_start:])
    return body


def decrypt_sample(data, account, codes, inspect_wxgf=False):
    """按三段模型与有限本机候选验证；私有格式分析单独标记。"""
    header = parse_header(data)
    if header["status"] != "ready":
        return header
    if not codes:
        return {**header, "status": "media_parameters_unavailable"}
    matched = set()
    padding_failed = False
    candidates = candidate_keys(account, codes)
    try:
        for rule, key, xor_byte in candidates:
            cipher = AES.new(key, AES.MODE_ECB)
            detected = signature(cipher.decrypt(data[15:31]))
            if detected is None:
                continue
            matched.add(detected)
            try:
                body = decrypt_body(data, header, cipher, xor_byte)
            except ValueError:
                padding_failed = True
                continue
            try:
                if detected == "WXGF":
                    details = {}
                    if inspect_wxgf:
                        from wxgf import inspect_payload

                        details = {"wxgf_analysis": inspect_payload(body)}
                    return {**header, **details, "status": "unsupported_payload", "outer_decrypted": True, "identity_rule": rule}
                details = validate_image(body)
                return {**header, **details, "status": "validated", "identity_rule": rule}
            except Exception:
                continue
            finally:
                body[:] = b"\0" * len(body)
    finally:
        candidates.close()
    status = "parameters_not_applicable"
    if matched:
        status = "invalid_padding" if padding_failed else "invalid_image"
    return {**header, "status": status, "matched_formats": sorted(matched)}
