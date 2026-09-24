from pathlib import Path
import itertools,os,random,subprocess,time,json
root=Path(__file__).resolve().parent
outdir=root/'linux-construction-results';outdir.mkdir(exist_ok=True)
(root/'linux-construction-host.txt').write_text(subprocess.check_output('date; uptime; lscpu',shell=True,text=True))
groups=list(itertools.product([(512,512),(3840,2160)],[1,6],['compare','candidate','pair']))
with (root/'linux-construction-runs.jsonl').open('w') as meta:
 for r in [1,2,3]:
  random.Random(197+r).shuffle(groups)
  for (w,h),t,scenario in groups:
   versions=['oldcc','newcc','oldcs','newcs','oldss','newss'];random.Random(w+h+t+r).shuffle(versions)
   for v in versions:
    out=outdir/f'{w}x{h}-t{t}-rgba8-{scenario}-{v}-r{r}.csv'
    env=dict(os.environ,RAYON_NUM_THREADS=str(t),MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
    cmd=['taskset','-c','2' if t==1 else '0-5',str(root/'modes/target/release/review-bench') if v.startswith('old') else str(root/'construction/target/release/review-bench'),str(w),str(h),'rgba8',v[-2:],scenario]
    start=time.time()
    with out.open('w') as f:subprocess.run(cmd,env=env,stdout=f,check=True)
    meta.write(json.dumps(dict(case=out.stem,start=start,end=time.time()))+'\n');meta.flush()
   print('DONE',r,w,h,t,scenario,flush=True)
