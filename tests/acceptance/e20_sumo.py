#!/usr/bin/env python3
"""Compare matched free-flow cases with Eclipse SUMO 1.28.0 (E20-S03)."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

VERSION = "1.28.0"
# Match the fixed E06 corridor length and independent SUMO integration step.
STEP_SECONDS = 0.1
TOLERANCE_SECONDS = 2 * STEP_SECONDS


def run(command):
    return subprocess.run(command, check=True, capture_output=True, text=True, timeout=30).stdout


def sumo_duration(work, netconvert, sumo, speed):
    nodes = work / "nodes.xml"
    nodes.write_text('<nodes><node id="a" x="0" y="0"/><node id="b" x="100" y="0"/></nodes>')
    edges = work / "edges.xml"
    edges.write_text(f'<edges><edge id="ab" from="a" to="b" speed="{speed}"/></edges>')
    network = work / "network.xml"
    run([netconvert, "--node-files", nodes, "--edge-files", edges, "--output-file", network])
    routes = work / "routes.xml"
    routes.write_text(
        '<routes><vType id="car" sigma="0" speedFactor="1" speedDev="0"/>'
        '<vehicle id="probe" type="car" depart="0" departPos="0" departSpeed="max" arrivalPos="max">'
        '<route edges="ab"/></vehicle></routes>'
    )
    trips = work / "trips.xml"
    run([sumo, "--net-file", network, "--route-files", routes, "--step-length", str(STEP_SECONDS),
         "--seed", "42", "--end", "60", "--tripinfo-output", trips])
    trip, = ET.parse(trips).getroot().findall("tripinfo")
    if trip.attrib["id"] != "probe" or float(trip.attrib["routeLength"]) != 100.0:
        raise RuntimeError("SUMO probe does not match the 100 m fixture")
    return float(trip.attrib["duration"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("rust_example", type=Path)
    parser.add_argument("--sumo", default="sumo")
    parser.add_argument("--netconvert", default="netconvert")
    args = parser.parse_args()
    for binary in (args.sumo, args.netconvert):
        version = run([binary, "--version"]).splitlines()[0].split()[-1]
        if version != VERSION:
            raise RuntimeError(f"{binary} must use Eclipse SUMO {VERSION}")
    fast = {int(speed): float(duration) for speed, duration in (
        line.split(",") for line in run([args.rust_example, "--independent-reference"]).splitlines()
    )}
    if set(fast) != {5, 10}:
        raise RuntimeError("missing E08 reference cases")
    cases = []
    with tempfile.TemporaryDirectory(prefix="carfree-e20-sumo-") as temp:
        for speed in (10, 5):
            detailed = sumo_duration(Path(temp), args.netconvert, args.sumo, speed)
            error = fast[speed] - detailed
            cases.append(dict(speed_mps=speed, fast_seconds=fast[speed], sumo_seconds=detailed,
                              signed_error_seconds=error))
    print(json.dumps({"sumo_version": VERSION, "step_seconds": STEP_SECONDS,
                      "tolerance_seconds": TOLERANCE_SECONDS, "cases": cases}, indent=2))
    if any(abs(case["signed_error_seconds"]) > TOLERANCE_SECONDS + 1e-9 for case in cases):
        raise SystemExit("free-flow comparison exceeded time-step tolerance")


if __name__ == "__main__":
    main()
