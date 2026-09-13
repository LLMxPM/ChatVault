"""验证微信图片的离线解密路径；仅输出脱敏统计，不输出密钥、路径或明文。"""

import argparse
import collections
import importlib.metadata
import json
import pathlib
import sys
import time

from codec import decrypt_sample
from source import account_directories, collect_samples, normal_entry, read_codes, read_stable


def probe_account(account, number, samples, seed, inspect_wxgf=False):
    """对账号执行有界分层抽样，失败信息仅输出固定状态码。"""
    chosen, population = collect_samples(account, samples, seed)
    codes, parameter_stats = read_codes()
    rows = []
    started = time.monotonic()
    try:
        for slot, (month, variant, path) in enumerate(chosen, 1):
            row = {"sample": f"S{slot:02}", "month": month, "variant": variant}
            if time.monotonic() - started > 60:
                rows.append({**row, "status": "account_time_limit"})
                continue
            try:
                data, error = read_stable(path)
                if error:
                    rows.append({**row, "status": error})
                    continue
                row["source_bytes"] = len(data)
                rows.append({**row, **decrypt_sample(data, account.name, codes, inspect_wxgf)})
            except Exception:
                rows.append({**row, "status": "sample_processing_error"})
    finally:
        codes.clear()
    counts = dict(collections.Counter(row["status"] for row in rows))
    return {"account": f"A{number}", "population": population, "parameter_sources": parameter_stats, "results": counts, "samples": rows}


def main():
    """验证显式指定的微信根；命令行只接收目录和样本数，不接收密钥。"""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--samples-per-account", type=int, default=48, choices=range(1, 65))
    parser.add_argument("--seed", type=int, default=20260913)
    parser.add_argument("--inspect-wxgf", action="store_true", help="在内存中分析私有容器，结果不计为普通图片备份成功")
    args = parser.parse_args()
    if args.root.name.lower() != "xwechat_files" or not normal_entry(args.root):
        raise ValueError("invalid_source_root")
    dependencies = ["pillow", "pycryptodome", "blake3"] + (["av"] if args.inspect_wxgf else [])
    print(json.dumps({"parser": "v2_aes_pkcs7_raw_xor", "seed": args.seed, "samples_per_account": args.samples_per_account, "dependencies": {name: importlib.metadata.version(name) for name in dependencies}}), flush=True)
    for number, account in enumerate(account_directories(args.root), 1):
        print(json.dumps(probe_account(account, number, args.samples_per_account, args.seed, args.inspect_wxgf)), flush=True)


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print(json.dumps({"status": "probe_failed_without_sensitive_details"}))
        sys.exit(1)
