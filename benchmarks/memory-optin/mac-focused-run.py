from pathlib import Path
import csv, hashlib, itertools, json, os, random, subprocess, threading, time
root=Path(__file__).resolve().parent
(root/'mac-focused-results').mkdir(exist_ok=True)
# Keep the actual source identities with the measurements, independent of Git state.
for v in ['base','modes']:
 h=hashlib.sha256()
 for p in sorted((root/('source-'+v)/'dssim-core/src').rglob('*.rs')):
  h.update(str(p.relative_to(root/('source-'+v))).encode());h.update(p.read_bytes())
 (root/f'{v}-source.sha256').write_text(h.hexdigest()+'\n')
(root/'mac-focused-host-before.txt').write_text(subprocess.check_output('date; uptime; sysctl hw.model hw.memsize hw.perflevel0.physicalcpu hw.perflevel1.physicalcpu; pmset -g therm; vm_stat',shell=True,text=True))
active_pid=None
samples=[]
stop=threading.Event()
lock=threading.Lock()
loadfile=(root/'mac-focused-load.jsonl').open('w')
def sample():
 global active_pid
 observed_pid=active_pid
 output=subprocess.check_output(['ps','-A','-o','pid=,pcpu=,comm='],text=True)
 entries=[]
 for line in output.splitlines():
  p=line.strip().split(None,2)
  if len(p)!=3:continue
  pid,cpu,cmd=int(p[0]),float(p[1]),p[2]
  if pid in [os.getpid(),observed_pid] or Path(cmd).name=='ps':continue
  if cpu:entries.append((pid,cpu,cmd))
 record={'time':time.time(),'active_pid':observed_pid,'other_cpu':sum(x[1] for x in entries),'top':sorted(entries,key=lambda x:x[1],reverse=True)[:8]}
 with lock:samples.append(record)
 loadfile.write(json.dumps(record)+'\n');loadfile.flush()
 return record
def monitor():
 while not stop.is_set():
  sample();stop.wait(0.5)
thread=threading.Thread(target=monitor,daemon=True);thread.start()
# Main matrix, then odd-size/continuity checks at six workers using RGBA8.
groups=[((512,512),6,'rgba8','candidate')]
versions=['oldcc','newcc']
meta=(root/'mac-focused-runs.jsonl').open('a')
try:
 for round_no in range(1,8):
  ordered=groups.copy();random.Random(197+round_no).shuffle(ordered)
  for gi,(shape,workers,fmt,scenario) in enumerate(ordered):
   w,h=shape
   order=versions.copy()
   order = versions[::-1] if round_no % 2 else versions.copy()
   for version in order:
    case=f'{w}x{h}-t{workers}-{fmt}-{scenario}-{version}-r{round_no}'
    out=root/'mac-focused-results'/(case+'.csv')
    exe=root/'modes/target/release/review-bench' if version.startswith('old') else root/'construction/target/release/review-bench'
    env=dict(os.environ,RAYON_NUM_THREADS=str(workers))
    args=[str(exe),str(w),str(h),fmt,version[-2:],scenario]
    for attempt in [1,2,3]:
     # Don't start work while another substantial CPU workload is active.
     deadline=time.monotonic()+60
     while sample()['other_cpu']>60 and time.monotonic()<deadline:
      print('WAIT busy host',case,flush=True);time.sleep(2)
     start=time.time()
     with out.open('w') as f:
      child=subprocess.Popen(args,env=env,stdout=f,stderr=subprocess.PIPE,text=True)
      active_pid=child.pid
      _,err=child.communicate()
     active_pid=None
     end=time.time()
     if child.returncode:raise RuntimeError(case+': '+err)
     with lock:observed=[x['other_cpu'] for x in samples if start<=x['time']<=end]
     peak=max(observed,default=0)
     noisy=peak>80
     record=dict(case=case,attempt=attempt,start=start,end=end,other_cpu_max=peak,noisy=noisy)
     meta.write(json.dumps(record)+'\n');meta.flush()
     if not noisy:break
     shutil_name=out.with_suffix(f'.attempt{attempt}.csv')
     out.replace(shutil_name)
     print('RETRY competing CPU',case,peak,flush=True)
    if not out.exists():
     raise RuntimeError('Host stayed busy for '+case)
   print('DONE',round_no,gi+1,len(ordered),f'{w}x{h}',workers,fmt,scenario,flush=True)
finally:
 stop.set();thread.join();loadfile.close();meta.close()
 (root/'mac-focused-host-after.txt').write_text(subprocess.check_output('date; uptime; pmset -g therm; vm_stat',shell=True,text=True))
