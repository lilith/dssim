"""Export immutable sources; generate identical standalone benchmark packages."""
from pathlib import Path
import hashlib, io, json, shutil, subprocess, sys, tarfile
here = Path(__file__).resolve().parent
repo = subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], cwd=here, text=True).strip()
root = Path(sys.argv[1]).resolve()
root.mkdir(parents=True, exist_ok=False)
revisions = {
    'main': '0e44c9b7fe91a5265c8e463436c28512186fe9cd',
    'dispatch': 'eb41ae307bfda358e41c022df2e2956fe5fd868c',
    'fusion': '27a478073263aef6fad9b095c466dc77a1b85033',
    'options': '331b56cf8998d89c3c89444880f8a0f51f046fb8',
}
(root/'revisions.json').write_text(json.dumps(revisions, indent=2)+'\n')
for name in ['bench.rs', 'run.py', 'memory.rs', 'build.sh']:
    shutil.copy2(here/name, root/name)
for version, revision in revisions.items():
    source = root/('source-'+version)
    source.mkdir()
    archive = subprocess.check_output(['git', 'archive', '--format=tar', revision], cwd=repo)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        tar.extractall(source, filter='data')
    harness = root/version
    harness.mkdir()
    (harness/'Cargo.toml').write_text('''[package]
name = "review-bench"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
dssim-core = { path = "../source-'''+version+'''/dssim-core" }
imgref = "=1.12.3"
rgb = "=0.8.53"
[[bin]]
name = "review-bench"
path = "../bench.rs"
[[bin]]
name = "memory"
path = "../memory.rs"
[features]
options = []
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 16
panic = "abort"
''')
    shutil.copy2(here/'Cargo.lock', harness/'Cargo.lock')
    hashes = {str(p.relative_to(source)):hashlib.sha256(p.read_bytes()).hexdigest()
              for p in sorted((source/'dssim-core').rglob('*.rs'))}
    (root/(version+'-source-sha256.json')).write_text(json.dumps(hashes, indent=2)+'\n')
print(root)
