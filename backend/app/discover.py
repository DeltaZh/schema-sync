import fnmatch


def match_databases(names: list[str], pattern: str, exclude: str | None = None) -> list[str]:
    out = [n for n in names if fnmatch.fnmatch(n, pattern)]
    if exclude is not None:
        out = [n for n in out if n != exclude]
    return sorted(out)
