#!/usr/bin/env python3
"""Print the X id of the largest caQtDM display window.

The version splash is titled "caQtDM V…" and sits on top of the display.
xwininfo -root -tree is read from stdin. The file stem is argv[1].
"""

import re
import sys

stem = sys.argv[1] if len(sys.argv) > 1 else ""
best = 0
best_id = ""
for line in sys.stdin:
    if "caQtDM" not in line or "caQtDM V" in line:
        continue
    wid = re.search(r"0x[0-9a-fA-F]+", line)
    geom = re.search(r"(\d+)x(\d+)", line)
    if wid is None or geom is None:
        continue
    if stem not in line and "MainWindow" not in line:
        continue
    area = int(geom.group(1)) * int(geom.group(2))
    if area > best:
        best = area
        best_id = wid.group(0)
if best_id:
    print(best_id)
