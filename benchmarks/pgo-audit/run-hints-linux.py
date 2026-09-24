import os,subprocess
from pathlib import Path
root=Path('/tmp/dssim-pgo-evidence')
variants={v:root/v/'target-base'/'x86_64-unknown-linux-gnu/release/review-bench' for v in ['main','pr','inline','caps']}
for t,cpus in [(1,'2'),(6,'0-5')]:
 for r in [1,2]:
  for name in list(variants)[::1 if r==1 else -1]:
   env=dict(os.environ,RAYON_NUM_THREADS=str(t),MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
   out=root/'results'/f'linux-hints-{name}-t{t}-r{r}.csv'
   with out.open('w') as f: subprocess.run(['taskset','-c',cpus,str(variants[name])],stdout=f,env=env,check=True)
   print(out.name,flush=True)
