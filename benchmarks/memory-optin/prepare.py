"""Export the exact three revisions and prepare standalone Rust 1.90 harnesses."""
from pathlib import Path
import io, shutil, subprocess, sys, tarfile
here = Path(__file__).resolve().parent
repo = subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], cwd=here, text=True).strip()
root = Path(sys.argv[1]).resolve()
root.mkdir(parents=True, exist_ok=False)
revisions = {
    'base': 'eb41ae307bfda358e41c022df2e2956fe5fd868c',
    'modes': 'f300435f330056a7707e39aa53bad014c34373e8',
    'construction': '1b891ad2597d70f21d90ffb874bac53fd00a37c9',
}
for name in ['baseline-bench.rs', 'baseline-parity.rs', 'run.py', 'linux-run.py', 'construction-run.py', 'linux-construction-run.py', 'mac-focused-run.py', 'linux-focused-run.py', 'check.py']:
    shutil.copy2(here/name, root/name)
# The original main matrix scripts expect this filename.
shutil.copy2(here/'baseline-bench.rs', root/'bench.rs')
for version, revision in revisions.items():
    source = root/('source-'+version)
    source.mkdir()
    archive = subprocess.check_output(['git', 'archive', '--format=tar', revision], cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        tar.extractall(source, filter='data')
    harness = root/version
    harness.mkdir()
    manifest = (here/'Cargo.toml').read_text().replace('../../dssim-core', '../source-'+version+'/dssim-core')
    if version != 'construction':
        manifest = manifest[:manifest.index('[[bin]]\nname = "memory"')]
        manifest = manifest.replace('path = "bench.rs"', 'path = "../baseline-bench.rs"').replace('path = "parity.rs"', 'path = "../baseline-parity.rs"')
    else:
        for name in ['bench.rs', 'parity.rs', 'memory.rs', 'score.rs']:
            shutil.copy2(here/name, harness/name)
    (harness/'Cargo.toml').write_text(manifest)
    shutil.copy2(here/'Cargo.lock', harness/'Cargo.lock')
    print('Prepared', version, revision)
