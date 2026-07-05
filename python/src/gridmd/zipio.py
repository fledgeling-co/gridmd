"""Deterministic ZIP I/O for the ``.xlsx`` package.

Writing uses STORE with a fixed DOS timestamp so output is byte-stable across
runs. Reading accepts both STORE and DEFLATE members (``zipfile`` inflates
DEFLATE via ``zlib``), so any peer implementation's compression choice imports
cleanly.
"""

from __future__ import annotations

import io
import zipfile

_FIXED_DATE_TIME = (1980, 1, 1, 0, 0, 0)


def zip_write(entries: list[tuple[str, bytes]]) -> bytes:
    """Write ``[(name, data), ...]`` to a deterministic STORE-only ZIP buffer."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_STORED) as zf:
        for name, data in entries:
            info = zipfile.ZipInfo(filename=name, date_time=_FIXED_DATE_TIME)
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o600 << 16
            zf.writestr(info, data)
    return buf.getvalue()


def zip_read(data: bytes) -> dict[str, bytes]:
    """Read a ZIP buffer into an ordered ``{name: bytes}`` map, verifying CRCs."""
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as zf:
            bad = zf.testzip()
            if bad is not None:
                raise ValueError(f"corrupt zip member: {bad}")
            return {info.filename: zf.read(info.filename) for info in zf.infolist()}
    except zipfile.BadZipFile as e:
        raise ValueError(f"not a valid zip/.xlsx package: {e}") from e
