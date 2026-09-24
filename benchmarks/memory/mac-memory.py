from pathlib import Path
import os,subprocess
import sys
root=Path(sys.argv[1]).resolve()
for t in [1,6]:
 for w,h in [(2049,1024),(4097,2048)]:
  for v in ['base','fused','stream']:
   out=root/'results'/f'mac-memory-{v}-{w}x{h}-t{t}.csv'
   with out.open('w') as f, out.with_suffix('.rss').open('w') as err:
    subprocess.run(['/usr/bin/time','-l',str(root/v/'target/release/memory'),'rgba',str(w),str(h)],stdout=f,stderr=err,env=dict(os.environ,RAYON_NUM_THREADS=str(t)),check=True)
   print(out.name,flush=True)
