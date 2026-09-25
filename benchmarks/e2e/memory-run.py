from pathlib import Path
import os, platform, subprocess, sys
root=Path(sys.argv[1]).resolve()
host='mac' if platform.system()=='Darwin' else 'linux'
out=root/('memory-'+host)
out.mkdir(exist_ok=True)
env=dict(os.environ,RAYON_NUM_THREADS='1')
for key in list(env):
    if key.startswith('MALLOC_'):env.pop(key)
for w,h in [(512,512),(3840,2160)]:
    for version,mode in [('main','cc'),('dispatch','cc'),('fusion','cc'),('options','ss'),('options','cs'),('options','cc')]:
        args=[str(root/version/'target/release/memory'),'rgba',str(w),str(h)]
        if host=='linux':args=['taskset','-c','2']+args
        text=subprocess.check_output(args,env=dict(env,DSSIM_MODE=mode),text=True)
        (out/f'{w}x{h}-{version}-{mode}.csv').write_text(text)
        print(host,w,h,version,mode,flush=True)
