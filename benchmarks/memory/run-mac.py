from pathlib import Path
import os,subprocess
import sys
root=Path(sys.argv[1]).resolve()
for t,cpus in [(1,'2'),(6,'0-5')]:
 env=dict(os.environ,RAYON_NUM_THREADS=str(t),MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
 for r in [1,2]:
  for v in ['base','fused','stream'][::1 if r==1 else -1]:
   out=root/'results'/f'mac-{v}-t{t}-r{r}.csv'
   with out.open('w') as f:subprocess.run([str(root/v/'target/release/review-bench')],env=env,stdout=f,check=True)
   print(out.name,flush=True)
