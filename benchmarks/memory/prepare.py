"""Prepare isolated, pinned worktrees and identical benchmark harnesses."""
from pathlib import Path
import shutil
import subprocess
import argparse

source = Path(__file__).resolve().parent
repo = source.parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('root', type=Path)
parser.add_argument('--historical', action='store_true', help='use the revisions measured in the original timing tables')
args = parser.parse_args()
root = args.root.resolve()
root.mkdir(parents=True, exist_ok=False)
(root / 'results').mkdir()
versions = {
    'base': 'eb41ae307bfda358e41c022df2e2956fe5fd868c',
    'fused': '27a478073263aef6fad9b095c466dc77a1b85033',
    'stream': '22e9ab033d5e5ac41ee961de00e7ae9bd959b634',
}
if args.historical:
    versions.update(fused='bc028852b1c6ba0c192ddbb77b924eda1e530f9b', stream='04a3f2b6803948cce6769d49a0481841c2087a15')
for version, commit in versions.items():
    checkout = root / (version + '-source')
    subprocess.run(['git', '-C', str(repo), 'worktree', 'add', '--detach', str(checkout), commit], check=True)
    harness = root / version
    harness.mkdir()
    for name in ['Cargo.toml', 'Cargo.lock', 'bench.rs', 'borrowed.rs', 'parity.rs', 'memory.rs', 'score.rs']:
        shutil.copy2(source / name, harness / name)
    manifest = harness / 'Cargo.toml'
    # POSIX paths; TOML literal string avoids escaping spaces/backslashes.
    path = str(checkout / 'dssim-core')
    if "'" in path:
        raise ValueError('Choose a working path without apostrophes')
    manifest.write_text(manifest.read_text().replace('"../../dssim-core"', "'" + path + "'"))
print(root)
