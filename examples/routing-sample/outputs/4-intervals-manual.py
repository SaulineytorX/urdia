def merge_intervals(intervals):
    if not isinstance(intervals, (list, tuple)):
        raise TypeError("intervals must be a list or tuple")

    validated = []
    for pair in intervals:
        if not isinstance(pair, (list, tuple)) or len(pair) != 2:
            raise TypeError("each interval must be a list or tuple of two ints")
        start, end = pair
        if (
            not isinstance(start, int)
            or isinstance(start, bool)
            or not isinstance(end, int)
            or isinstance(end, bool)
        ):
            raise TypeError("interval endpoints must be ints, excluding bool")
        if start > end:
            raise ValueError("interval start must not exceed end")
        validated.append((start, end))

    validated.sort()
    merged = []
    for start, end in validated:
        if merged and start <= merged[-1][1]:
            previous_start, previous_end = merged[-1]
            merged[-1] = (previous_start, max(previous_end, end))
        else:
            merged.append((start, end))
    return merged
