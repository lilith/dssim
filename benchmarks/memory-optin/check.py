"""Run separately from timing: exact output comparisons, heap counters and RSS."""
from pathlib import Path
import filecmp, hashlib, json, os, platform, subprocess, sys
root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parent
host = 'mac' if platform.system() == 'Darwin' else 'linux'
out = root/'validation'; out.mkdir(exist_ok=True)
env = dict(os.environ, RAYON_NUM_THREADS='1')
if host == 'linux':
    env.update(MALLOC_MMAP_THRESHOLD_='67108864', MALLOC_TRIM_THRESHOLD_='2147483647')
records = []
for mode in ['base', 'cc', 'cs', 'sc', 'ss']:
    exe = root/('base' if mode == 'base' else 'construction')/'target/release/parity'
    bits = out/f'{host}-{mode}.bits'; scores = out/f'{host}-{mode}.scores'
    with bits.open('wb') as stdout, scores.open('wb') as stderr:
        subprocess.run([str(exe)], env=dict(env, DSSIM_MODE=mode), stdout=stdout, stderr=stderr, check=True)
    if mode != 'base':
        assert filecmp.cmp(bits, out/f'{host}-base.bits', shallow=False), mode
        assert filecmp.cmp(scores, out/f'{host}-base.scores', shallow=False), mode
    with bits.open('rb') as f:
        digest = hashlib.file_digest(f, 'sha256').hexdigest()
    records.append(dict(mode=mode, bytes=bits.stat().st_size, sha256=digest))
(out/f'{host}-parity.json').write_text(json.dumps(records, indent=2)+'\n')
for w,h in [(512,512),(3840,2160)]:
    for mode in ['cc','cs','ss']:
        tag=f'{host}-{w}x{h}-{mode}'
        time_args = ['-l'] if host == 'mac' else ['-f','max_rss_kib,%M']
        cmd = ['/usr/bin/time', *time_args, str(root/'construction/target/release/memory'), 'rgba', str(w), str(h)]
        with (out/(tag+'.csv')).open('w') as stdout, (out/(tag+'.rss')).open('w') as stderr:
            subprocess.run(cmd, env=dict(env, DSSIM_MODE=mode), stdout=stdout, stderr=stderr, check=True)
print(host, 'all four cache policies match baseline bytes and scores')
