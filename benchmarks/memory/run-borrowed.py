from pathlib import Path
import os,subprocess,sys
root=Path(sys.argv[1]).resolve()
host='mac' if sys.platform=='darwin' else 'linux'
for t,cpus in [(1,'2'),(6,'0-5')]:
 env=dict(os.environ,RAYON_NUM_THREADS=str(t))
 if host=='linux':
  env.update(MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
 for r in [1,2,3]:
  for v in ['base','stream'][::1 if r%2 else -1]:
   out=root/'results'/f'borrowed-{host}-{v}-t{t}-r{r}.csv'
   cmd=[str(root/v/'target/release/borrowed')]
   if host=='linux':cmd=['taskset','-c',cpus]+cmd
   with out.open('w') as f:subprocess.run(cmd,env=env,stdout=f,check=True)
   print(out.name,flush=True)
