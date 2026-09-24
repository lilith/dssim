from pathlib import Path
import os,subprocess
import sys
root=Path(sys.argv[1]).resolve()
for t,cpus in [(1,'2'),(6,'0-5')]:
 env=dict(os.environ,RAYON_NUM_THREADS=str(t),MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
 for r in [1,2]:
  for v in ['base','fused','stream'][::1 if r==1 else -1]:
   out=root/'results'/f'linux-{v}-t{t}-r{r}.csv'
   with out.open('w') as f:subprocess.run(['taskset','-c',cpus,str(root/v/'target/release/review-bench')],env=env,stdout=f,check=True)
   print(out.name,flush=True)
 for w,h in [(2049,1024),(4097,2048)]:
  for v in ['base','fused','stream']:
   out=root/'results'/f'linux-memory-{v}-{w}x{h}-t{t}.csv'
   with out.open('w') as f:subprocess.run(['/usr/bin/time','-f','rss_max_kib,%M','-o',str(out.with_suffix('.rss')), 'taskset','-c',cpus,str(root/v/'target/release/memory'),'rgba',str(w),str(h)],env=env,stdout=f,check=True)
   print(out.name,flush=True)
