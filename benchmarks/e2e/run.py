"""Run paired, randomized benchmark rounds; retain batches and load rejections."""
from pathlib import Path
import csv, hashlib, itertools, json, os, platform, random, subprocess, threading, time
root = Path(__file__).resolve().parent
host = 'mac' if platform.system() == 'Darwin' else 'linux'
out = root/('results-'+host)
out.mkdir(exist_ok=True)
# Full-pair controls isolate dispatch and fusion. Other rows focus on cache policy.
cases = {
    'pair': [('main','cc'), ('dispatch','cc'), ('fusion','cc'), ('options','ss'), ('options','cc')],
    'candidate': [('main','cc'), ('dispatch','cc'), ('options','cs'), ('options','cc')],
}
clean_env = dict(os.environ)
for k in list(clean_env):
    if k.startswith(('MALLOC_', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_PROFILE_', 'CARGO_TARGET_')):
        clean_env.pop(k)
metadata = {'host':host, 'platform':platform.platform(), 'time':time.time(),
            'revisions':json.loads((root/'revisions.json').read_text()),
            'allocator':'system defaults', 'warmup_ms':100, 'batch_ms':100, 'batches':7,
            'rounds':3, 'phase_timing':True, 'inputs':'RGBA8 reference and four rotating candidates', 'linux_affinity':{'1':'2','6':'0-5'}}
metadata['rustc'] = subprocess.check_output([str(Path.home()/'.cargo/bin/rustc'), '+1.90.0', '-Vv'],text=True)
metadata['binaries'] = {v:hashlib.sha256((root/v/'target/release/review-bench').read_bytes()).hexdigest() for v in metadata['revisions']}
if host=='mac':
    metadata['hardware']=subprocess.check_output(['sysctl','hw.model','hw.memsize','hw.physicalcpu','hw.perflevel0.physicalcpu','hw.perflevel1.physicalcpu'],text=True)
else:
    metadata['hardware']=subprocess.check_output(['lscpu'],text=True)
(out/'metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
active_pid = None
stop = threading.Event()
lock = threading.Lock()
samples = []
loadfile = (out/'load.jsonl').open('a')
previous = {}
previous_time = time.monotonic()
def load():
    global previous, previous_time
    ignored = {os.getpid(), active_pid}
    if host=='mac':
        raw = subprocess.check_output(['ps','-A','-o','pid=,pcpu=,comm='],text=True)
        total = 0.
        for line in raw.splitlines():
            p=line.strip().split(None,2)
            if len(p)==3 and int(p[0]) not in ignored and Path(p[2]).name not in ('ps','review-bench'):
                total += float(p[1])
    else:
        now=time.monotonic(); current={}
        for path in Path('/proc').glob('[0-9]*/stat'):
            try:
                fields=path.read_text().rsplit(')',1)[1].split()
                current[int(path.parent.name)] = int(fields[11])+int(fields[12])
            except (OSError,ValueError,IndexError): pass
        total=sum(max(0,ticks-previous.get(pid,ticks)) for pid,ticks in current.items() if pid not in ignored)/os.sysconf('SC_CLK_TCK')/max(.001,now-previous_time)*100
        previous,current = current,previous
        previous_time=now
    record={'time':time.time(),'other_cpu':total,'active_pid':active_pid}
    with lock: samples.append(record)
    loadfile.write(json.dumps(record)+'\n');loadfile.flush()
    return total

def monitor():
    while not stop.is_set():
        load();stop.wait(.25)
thread=threading.Thread(target=monitor,daemon=True);thread.start()
runs=(out/'runs.jsonl').open('a')
groups=list(itertools.product([(512,512),(3840,2160)],[1,6],cases))
try:
    for round_no in [1,2,3]:
        ordered=groups.copy();random.Random(9200+round_no).shuffle(ordered)
        for gi,((w,h),workers,scenario) in enumerate(ordered):
            versions=cases[scenario].copy();random.Random(9300+round_no*100+gi).shuffle(versions)
            for version,mode in versions:
                case=f'{w}x{h}-t{workers}-{scenario}-{version}-{mode}-r{round_no}'
                path=out/(case+'.csv')
                if path.exists(): continue
                args=[str(root/version/'target/release/review-bench'),str(w),str(h),mode,scenario]
                if host=='linux': args=['taskset','-c','2' if workers==1 else '0-5']+args
                env=dict(clean_env,RAYON_NUM_THREADS=str(workers))
                for attempt in range(1,7):
                    waited=0
                    while True:
                        with lock: recent=samples[-4:]
                        if len(recent)>=4 and max(x['other_cpu'] for x in recent)<60: break
                        if waited%15==0: print('WAIT busy host',case,flush=True)
                        time.sleep(1);waited+=1
                        if waited>=600: raise RuntimeError('Competing load persists; resume later')
                    start=time.time()
                    # Exclude the benchmark's PID while sampling background load.
                    child=subprocess.Popen(args,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
                    active_pid=child.pid
                    stdout,stderr=child.communicate()
                    end=time.time();active_pid=None
                    if child.returncode: raise RuntimeError(case+': '+stderr)
                    with lock: observed=[s['other_cpu'] for s in samples if start+.25<=s['time']<=end-.25]
                    peak=max(observed,default=0)
                    rejected=peak>80
                    record=dict(case=case,attempt=attempt,start=start,end=end,other_cpu_max=peak,rejected=rejected)
                    runs.write(json.dumps(record)+'\n');runs.flush()
                    if not rejected:
                        path.write_text(stdout);break
                    (out/(case+f'.rejected{attempt}.csv')).write_text(stdout)
                    print('RETRY',case,round(peak,1),flush=True)
                if not path.exists(): raise RuntimeError('Repeated competing load: '+case)
            print('DONE',round_no,gi+1,len(ordered),f'{w}x{h}',workers,scenario,flush=True)
finally:
    stop.set();thread.join();loadfile.close();runs.close()
