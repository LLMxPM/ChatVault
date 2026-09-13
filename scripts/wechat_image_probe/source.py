"""微信图片只读抽样的目录白名单、分层选样和源文件稳定性验证。"""

import collections
import os
import pathlib
import random
import re
import stat

from blake3 import blake3

MAX_BYTES = 32 * 1024 * 1024
PARAMETER_DIRS = ("Tencent/xwechat/net/kvcomm", "Tencent/xwechat/ilink/kvcomm")


def normal_entry(path):
    """拒绝 Windows 重解析点与符号链接，避免遍历到授权根外。"""
    try:
        info = path.lstat()
        return not (info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT)
    except OSError:
        return False


def account_directories(root):
    """只枚举给定微信根下包含附件目录的账号，稳定排序但不输出账号名。"""
    accounts = []
    for path in sorted(root.iterdir()):
        ancestors = (path, path / "msg", path / "msg" / "attach")
        if all(item.is_dir() and normal_entry(item) for item in ancestors):
            accounts.append(path)
    return accounts


def collect_samples(account, count, seed):
    """按月份及后缀分层轮转抽样，层内固定种子随机选取。"""
    groups = collections.defaultdict(list)
    root = account / "msg" / "attach"
    for conversation in sorted(root.iterdir()):
        if not conversation.is_dir() or not normal_entry(conversation):
            continue
        for month in sorted(conversation.iterdir()):
            if not re.fullmatch(r"\d{4}-(0[1-9]|1[0-2])", month.name):
                continue
            image_dir = month / "Img"
            if not all(p.is_dir() and normal_entry(p) for p in (month, image_dir)):
                continue
            for path in sorted(image_dir.iterdir()):
                if path.suffix.lower() != ".dat" or not path.is_file() or not normal_entry(path):
                    continue
                variant = "thumbnail" if path.stem.endswith("_t") else "high" if path.stem.endswith("_h") else "display"
                groups[(month.name, variant)].append(path)
    population = [{"month": key[0], "variant": key[1], "count": len(paths)} for key, paths in sorted(groups.items())]
    rng = random.Random(seed)
    for paths in groups.values():
        rng.shuffle(paths)
    chosen = []
    while len(chosen) < count and any(groups.values()):
        for (month, variant), paths in sorted(groups.items()):
            if paths and len(chosen) < count:
                chosen.append((month, variant, paths.pop()))
    return chosen, population


def read_codes():
    """仅从当前用户白名单统计文件名提取参数，报告只包含数量。"""
    appdata = pathlib.Path(os.environ["APPDATA"])
    codes, directory_stats = set(), []
    for relative in PARAMETER_DIRS:
        directory = appdata / relative
        found = 0
        try:
            ancestors = [directory, *list(directory.parents)[:3]]
            if directory.is_dir() and all(normal_entry(path) for path in ancestors):
                for path in directory.iterdir():
                    match = re.fullmatch(r"key_(\d{1,10})_.+\.statistic", path.name)
                    if match and path.is_file() and normal_entry(path):
                        codes.add(int(match.group(1)))
                        found += 1
                    if len(codes) > 128:
                        raise RuntimeError("candidate_limit_exceeded")
            directory_stats.append({"directory": relative, "candidate_files": found})
        except OSError:
            directory_stats.append({"directory": relative, "error": "directory_unavailable"})
    return codes, directory_stats


def read_stable(path):
    """两次只读比较 BLAKE3 及 stat；密文不另存，摘要不进入输出。"""
    before = path.stat()
    if before.st_size > MAX_BYTES:
        return None, "source_resource_limit"
    with path.open("rb") as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        return None, "source_resource_limit"
    digest = blake3(data).digest()
    with path.open("rb") as stream:
        second = blake3()
        read_count = 0
        while chunk := stream.read(65536):
            read_count += len(chunk)
            if read_count > MAX_BYTES:
                return None, "source_resource_limit"
            second.update(chunk)
    after = path.stat()
    original = (before.st_size, before.st_mtime_ns, before.st_ino)
    current = (after.st_size, after.st_mtime_ns, after.st_ino)
    if original != current or digest != second.digest() or len(data) != before.st_size:
        return None, "source_changed"
    return data, None
