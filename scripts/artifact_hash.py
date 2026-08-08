#!/usr/bin/env python3
"""Deterministic SHA-256 helpers for evidence files and directory bundles."""

from __future__ import annotations

import hashlib
import os
import stat
from pathlib import Path


TREE_HASH_SCHEME = "tree-sha256-v1"
_TREE_HASH_DOMAIN = b"gui-ecosystem-artifact-tree-v1"


def _update_field(digest, value: bytes) -> None:
    """Add one unambiguous length-prefixed field to a digest."""

    digest.update(len(value).to_bytes(8, "big"))
    digest.update(value)


def file_sha256(path: Path) -> str:
    """Return the conventional SHA-256 of one regular file's bytes."""

    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def directory_sha256(path: Path) -> str:
    """Hash a directory tree without host-specific filesystem metadata.

    Entries are ordered by their POSIX-style relative path. Each entry hashes
    its type and relative path; regular files additionally hash their byte
    length and contents, while symlinks hash their stored target. Timestamps,
    ownership, permissions, inode numbers, and traversal/creation order are
    deliberately excluded.
    """

    digest = hashlib.sha256()
    _update_field(digest, _TREE_HASH_DOMAIN)
    entries = sorted(
        path.rglob("*"), key=lambda entry: entry.relative_to(path).as_posix()
    )
    for entry in entries:
        relative = entry.relative_to(path).as_posix().encode("utf-8")
        mode = entry.lstat().st_mode
        if stat.S_ISLNK(mode):
            kind = b"symlink"
            payload = os.fsencode(os.readlink(entry))
        elif stat.S_ISDIR(mode):
            kind = b"directory"
            payload = b""
        elif stat.S_ISREG(mode):
            kind = b"file"
            payload = None
        else:
            raise ValueError(f"unsupported artifact entry type: {entry}")

        _update_field(digest, kind)
        _update_field(digest, relative)
        if payload is not None:
            _update_field(digest, payload)
            continue

        size = entry.stat().st_size
        digest.update(size.to_bytes(8, "big"))
        with entry.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
    return digest.hexdigest()


def artifact_sha256(path: Path) -> str:
    """Hash a regular file conventionally or a directory as a normalized tree."""

    if path.is_file():
        return file_sha256(path)
    if path.is_dir():
        return directory_sha256(path)
    raise ValueError(f"artifact is neither a regular file nor a directory: {path}")


def artifact_hash_scheme(path: Path) -> str:
    """Describe the convention used by :func:`artifact_sha256`."""

    if path.is_file():
        return "file-sha256"
    if path.is_dir():
        return TREE_HASH_SCHEME
    raise ValueError(f"artifact is neither a regular file nor a directory: {path}")
