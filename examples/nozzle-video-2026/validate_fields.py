#!/usr/bin/env python3
"""Check ASCII internal fields; these checks do not establish CFD convergence."""
import argparse
import json
import math
from pathlib import Path
import re


def internal_values(path, components):
    text = path.read_text()
    # Deliberately ignore wedge boundary vectors: radial face transforms have
    # nonzero z components even when the mid-plane solution has no swirl.
    section = re.search(r'internalField\s+nonuniform\s+List<\w+>\s+(\d+)\s*\((.*?)\n\)', text, re.S)
    if not section:
        raise ValueError(f'Expected an ASCII nonuniform internal field: {path}')
    values = [float(v) for v in re.findall(r'[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?', section[2])]
    count = int(section[1])
    if len(values) != components*count or not all(map(math.isfinite, values)):
        raise ValueError(f'Invalid field count or non-finite values: {path}')
    return values


def validate(folder):
    velocity = internal_values(folder/'U', 3)
    max_uz = max(map(abs, velocity[2::3]))
    if max_uz > 1e-12:
        raise ValueError(f'Unintended azimuthal velocity: max |Uz| = {max_uz} m/s')
    report = {'cells': len(velocity)//3, 'max_abs_internal_Uz_m_s': max_uz,
              'scope': 'field admissibility and no swirl only; convergence remains to be checked'}
    for name in ('T', 'rho', 'p'):
        values = internal_values(folder/name, 1)
        if len(values) != report['cells'] or min(values) <= 0:
            raise ValueError(f'Non-positive or incompatible {name} field')
        report[name] = {'min': min(values), 'max': max(values)}
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('time_directory', type=Path)
    print(json.dumps(validate(parser.parse_args().time_directory), indent=2))
