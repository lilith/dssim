from pathlib import Path
import itertools,json,os,platform,random,subprocess,threading,time
root=Path(__file__).resolve().parent
host='mac' if platform.system()=='Darwin' else 'linux'
out=root/(host+'-results');out.mkdir(exist_ok=True)
info='date; uptime; sysctl hw.model hw.memsize; pmset -g therm; vm_stat' if host=='mac' else 'date; uptime; lscpu'
(root/(host+'-before.txt')).write_text(subprocess.check_output(info,shell=True,text=True))
active_pid=None;samples=[];lock=threading.Lock();stop=threading.Event()
load=(root/(host+'-load.jsonl')).open('w')
def sample():
 observed_pid=active_pid
 text=subprocess.check_output(['ps','-A','-o','pid=,pcpu=,comm='],text=True)
 entries=[]
 for line in text.splitlines():
  p=line.strip().split(None,2)
  if len(p)!=3:continue
  pid,cpu,cmd=int(p[0]),float(p[1]),p[2]
  if pid in [os.getpid(),observed_pid] or Path(cmd).name=='ps':continue
  if cpu:entries.append((pid,cpu,cmd))
 record=dict(time=time.time(),active_pid=observed_pid,other_cpu=sum(x[1] for x in entries),top=sorted(entries,key=lambda x:x[1],reverse=True)[:8])
 with lock:samples.append(record)
 load.write(json.dumps(record)+'\n');load.flush()
 return record
def monitor():
 while not stop.is_set():sample();stop.wait(0.5)
thread=threading.Thread(target=monitor,daemon=True);thread.start()
threads=[1,6,12] if host=='mac' else [1,6]
groups=[]
for (w,h),t,scenario in itertools.product([(256,256),(512,512),(3840,2160)],threads,['compare','candidate','pair']):
 groups.append((w,h,t,scenario,'current',[1024,2048,4096,8192]))
for (w,h),t,scenario in itertools.product([(512,512),(3840,2160)],threads,['compare','pair']):
 groups.append((w,h,t,scenario,'pr197',[1024,4096]))
# Same chunk A/B in the old context-configuration API, for the regression probe.
for (w,h),scenario in itertools.product([(512,512),(3840,2160)],['candidate']):
 groups.append((w,h,6,scenario,'modes',[1024,4096]))
meta=(root/(host+'-runs.jsonl')).open('w')
try:
 for r in [1,2,3]:
  order=groups.copy();random.Random(1970+r).shuffle(order)
  for gi,(w,h,t,scenario,lineage,chunks) in enumerate(order):
   versions=chunks.copy();random.Random(w+h+t+197*r+gi).shuffle(versions)
   for n in versions:
    case=f'{w}x{h}-t{t}-{scenario}-{lineage}-n{n}-r{r}';path=out/(case+'.csv')
    env=dict(os.environ,RAYON_NUM_THREADS=str(t))
    cmd=[str(root/'bin'/f'{lineage}-{n}'),str(w),str(h),'rgba8','cc',scenario]
    if host=='linux':
     env.update(MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
     cmd=['taskset','-c','2' if t==1 else '0-5']+cmd
    for attempt in [1,2,3]:
     deadline=time.monotonic()+60
     while sample()['other_cpu']>60 and time.monotonic()<deadline:
      print('WAIT',case,flush=True);time.sleep(2)
     start=time.time()
     with path.open('w') as f:
      child=subprocess.Popen(cmd,env=env,stdout=f,stderr=subprocess.PIPE,text=True)
      active_pid=child.pid;_,err=child.communicate()
     active_pid=None;end=time.time()
     if child.returncode:raise RuntimeError(case+': '+err)
     with lock:observed=[x['other_cpu'] for x in samples if start<=x['time']<=end]
     peak=max(observed,default=0);noisy=peak>80
     meta.write(json.dumps(dict(case=case,attempt=attempt,start=start,end=end,other_cpu_max=peak,noisy=noisy))+'\n');meta.flush()
     if not noisy:break
     path.replace(path.with_suffix(f'.attempt{attempt}.csv'))
     print('RETRY',case,peak,flush=True)
    if not path.exists():raise RuntimeError('Host stayed busy: '+case)
   print('DONE',r,gi+1,len(order),w,h,t,scenario,lineage,flush=True)
finally:
 stop.set();thread.join();load.close();meta.close()
 (root/(host+'-after.txt')).write_text(subprocess.check_output(info,shell=True,text=True))
