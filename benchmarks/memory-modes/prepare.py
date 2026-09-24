"""Prepare pinned baseline and proposal worktrees with identical timing inputs."""
from pathlib import Path
import shutil
import subprocess
import sys

source = Path(__file__).resolve().parent
repo = source.parents[1]
root = Path(sys.argv[1]).resolve()
root.mkdir(parents=True, exist_ok=False)
(root / 'results').mkdir()
for version, commit in {
    'base': 'eb41ae307bfda358e41c022df2e2956fe5fd868c',
    'modes': 'f300435f330056a7707e39aa53bad014c34373e8',
}.items():
    checkout = root / (version + '-source')
    subprocess.run(['git', '-C', str(repo), 'worktree', 'add', '--detach', str(checkout), commit], check=True)
    harness = root / version
    harness.mkdir()
    for name in ['Cargo.toml', 'Cargo.lock', 'bench.rs', 'parity.rs', 'memory.rs', 'score.rs']:
        original = ('baseline-' + name) if version == 'base' and name in ['bench.rs', 'parity.rs', 'memory.rs'] else name
        shutil.copyfile(source / original, harness / name)
    manifest = harness / 'Cargo.toml'
    path = str(checkout / 'dssim-core')
    if "'" in path:
        raise ValueError('Choose a working path without apostrophes')
    manifest.write_text(manifest.read_text().replace('"../../dssim-core"', "'" + path + "'"))
print(root)
