"""用自行生成的图片验证抽样器，包括真实解码、错误参数与损坏尾部。"""

import hashlib
import io
import random
import struct
import unittest

from Crypto.Cipher import AES
from Crypto.Util.Padding import pad
from PIL import Image

from codec import MAGIC, account_variants, decrypt_body, decrypt_sample, parse_header, validate_image

DEMO_ACCOUNT = "wxid_probe"
DEMO_CODE = 424242


def encode_demo(body, aes_length=1024, xor_length=None):
    """用虚构参数构造 PKCS#7、明文中段及 XOR 尾段测试容器。"""
    key = hashlib.md5(f"{DEMO_CODE}{DEMO_ACCOUNT}".encode()).hexdigest()[:16].encode()
    cipher = AES.new(key, AES.MODE_ECB)
    xor_length = len(body) - aes_length if xor_length is None else xor_length
    split = len(body) - xor_length
    header = MAGIC + struct.pack("<II", aes_length, xor_length) + b"\x01"
    return header + cipher.encrypt(pad(body[:aes_length], 16)) + body[aes_length:split] + bytes(value ^ (DEMO_CODE & 255) for value in body[split:])


def generate_image(kind):
    """生成确定性的无用户内容图像，GIF 使用两帧验证完整帧遍历。"""
    image = Image.frombytes("RGB", (80, 80), random.Random(7).randbytes(80 * 80 * 3))
    stream = io.BytesIO()
    if kind == "GIF":
        other = Image.frombytes("RGB", (80, 80), random.Random(8).randbytes(80 * 80 * 3))
        image.save(stream, kind, save_all=True, append_images=[other])
    else:
        image.save(stream, kind)
    return stream.getvalue()


class ProbeTests(unittest.TestCase):
    """只运行必要的正反例，防止文件头命中被错误算为完整解密。"""

    def test_known_plaintext_roundtrip(self):
        """四种标准格式逐字节往返一致，且验证器能遍历动画帧。"""
        for kind in ("JPEG", "PNG", "GIF", "WEBP"):
            with self.subTest(format=kind):
                body = generate_image(kind)
                self.assertGreater(len(body), 1024)
                data = encode_demo(body)
                key = hashlib.md5(f"{DEMO_CODE}{DEMO_ACCOUNT}".encode()).hexdigest()[:16].encode()
                self.assertEqual(decrypt_body(data, parse_header(data), AES.new(key, AES.MODE_ECB), DEMO_CODE & 255), body)
                result = decrypt_sample(data, DEMO_ACCOUNT, {DEMO_CODE})
                self.assertEqual(result["status"], "validated")
                self.assertEqual(result["frames"], 2 if kind == "GIF" else 1)

    def test_wrong_parameters(self):
        """错误候选不应被识别为已验证图片。"""
        result = decrypt_sample(encode_demo(generate_image("PNG")), DEMO_ACCOUNT, {DEMO_CODE + 1})
        self.assertEqual(result["status"], "parameters_not_applicable")

    def test_middle_and_unaligned_aes(self):
        """非整块头、空尾段及真实中间段均需逐字节恢复。"""
        body = generate_image("PNG")
        for aes_length, xor_length in ((1024, 100), (1023, 100), (1024, 0)):
            with self.subTest(aes_length=aes_length, xor_length=xor_length):
                data = encode_demo(body, aes_length, xor_length)
                key = hashlib.md5(f"{DEMO_CODE}{DEMO_ACCOUNT}".encode()).hexdigest()[:16].encode()
                self.assertEqual(decrypt_body(data, parse_header(data), AES.new(key, AES.MODE_ECB), DEMO_CODE & 255), body)
                self.assertEqual(decrypt_sample(data, DEMO_ACCOUNT, {DEMO_CODE})["status"], "validated")

    def test_overlapping_segments(self):
        """声明的尾段超出边界时拒绝，不读取重叠片段。"""
        data = bytearray(encode_demo(generate_image("PNG")))
        data[10:14] = struct.pack("<I", len(data))
        self.assertEqual(decrypt_sample(data, DEMO_ACCOUNT, {DEMO_CODE})["status"], "unsupported_structure")

    def test_padding_is_verified(self):
        """原来被跳过的填充密文被破坏时必须拒绝，不能只看图像首部。"""
        data = bytearray(encode_demo(generate_image("PNG")))
        data[15 + 1024 : 15 + 1040] = bytes(16)
        self.assertEqual(decrypt_sample(data, DEMO_ACCOUNT, {DEMO_CODE})["status"], "invalid_padding")

    def test_custom_account_suffix(self):
        """只去掉公开规则中的四位十六进制后缀，保留其他真实名字。"""
        self.assertIn(("documented_hex_suffix_cleanup", "custom_name"), account_variants("custom_name_a1b2"))
        self.assertEqual(account_variants("custom_name_zzzz"), [("full_directory_name", "custom_name_zzzz")])

    def test_corrupt_tail(self):
        """首部仍正确的损坏 PNG 必须失败。"""
        body = generate_image("PNG")[:-20] + bytes(20)
        result = decrypt_sample(encode_demo(body), DEMO_ACCOUNT, {DEMO_CODE})
        self.assertEqual(result["status"], "invalid_image")

    def test_truncated_animation(self):
        """截断第二帧不可仅凭第一帧算成功。"""
        body = generate_image("GIF")[:-100] + b";"
        with self.assertRaises(Exception):
            validate_image(body)


if __name__ == "__main__":
    unittest.main()
