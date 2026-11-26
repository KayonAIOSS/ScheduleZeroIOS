"""Prints the UDID of an available iPhone simulator on the newest iOS runtime (prefers iPhone 16e)."""
import json
import subprocess
import sys

data = json.loads(subprocess.check_output(["xcrun", "simctl", "list", "devices", "available", "-j"]))


def version(runtime: str):
    # com.apple.CoreSimulator.SimRuntime.iOS-18-2 -> (18, 2)
    tail = runtime.rsplit("iOS-", 1)[-1]
    return tuple(int(p) for p in tail.split("-") if p.isdigit())


best = None
for runtime, devices in data["devices"].items():
    if ".iOS-" not in runtime:
        continue
    for d in devices:
        if not d["name"].startswith("iPhone"):
            continue
        score = (version(runtime), d["name"] == "iPhone 16e", "Pro" not in d["name"])
        if best is None or score > best[0]:
            best = (score, d["udid"], d["name"], runtime)

if best is None:
    sys.exit("no iPhone simulator available")
sys.stderr.write("simulator: %s (%s)\n" % (best[2], best[3]))
print(best[1])
