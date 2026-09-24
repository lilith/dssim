from pathlib import Path
import os,subprocess
root=Path(__file__).resolve().parent
out=root/'linux-focused-results';out.mkdir(exist_ok=True)
env=dict(os.environ,RAYON_NUM_THREADS='6',MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
for r in range(1,8):
 for v in (['new','old'] if r%2 else ['old','new']):
  exe=root/('modes' if v=='old' else 'construction')/'target/release/review-bench'
  with (out/f'3840x2160-t6-rgba8-pair-{v}cc-r{r}.csv').open('w') as f:
   subprocess.run(['taskset','-c','0-5',str(exe),'3840','2160','rgba8','cc','pair'],env=env,stdout=f,check=True)
 print('DONE',r,flush=True)
