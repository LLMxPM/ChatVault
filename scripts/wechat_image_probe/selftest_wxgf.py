"""验证 WXGF 诊断的分片边界与标准解码错误处理，不读取真实图片。"""

import unittest

from wxgf import inspect_payload, locate_partitions


def fake_container(streams):
    """构造仅用于边界测试的虚构分片容器；载荷不是实际 HEVC。"""
    header = bytearray(b"wxgf" + bytes([15]) + b"\0\x02\0\x10\0\x10" + bytes(4))
    for stream in streams:
        header.extend(len(stream).to_bytes(4, "big"))
        header.extend(stream)
    return header


class WxgfProbeTests(unittest.TestCase):
    """候选定位不能越界，多分片不能被自动解释成动画或最大分片。"""

    def test_multiple_partitions_retained(self):
        """保留不同大小的所有分片，不能直接丢弃疑似透明度分片。"""
        small = b"\0\0\0\1" + bytes(12)
        large = b"\0\0\1" + bytes(40)
        parts = locate_partitions(fake_container([small, large]))
        self.assertEqual([size for _, size in parts], [len(small), len(large)])

    def test_truncated_partition_rejected(self):
        """分片声明长度超出正文时不得返回越界切片。"""
        with self.assertRaises(ValueError):
            locate_partitions(fake_container([b"\0\0\0\1" + bytes(12)])[:-1])

    def test_invalid_hevc_does_not_count_as_image(self):
        """有边界与起始码的垃圾流不能冒充已验证图片。"""
        result = inspect_payload(fake_container([b"\0\0\0\1" + bytes(12)]))
        self.assertEqual(result["status"], "wxgf_analysis_incomplete")


if __name__ == "__main__":
    unittest.main()
