from pathlib import Path
import os,subprocess,sys,hashlib
root=Path(sys.argv[1]).resolve()
host='mac' if sys.platform=='darwin' else 'linux'
base=root/'results'/f'{host}-base.bits'
basescores=base.with_suffix('.scores')
with base.open('wb') as f, basescores.open('wb') as err:
 subprocess.run([str(root/'base/target/release/parity')], env=dict(os.environ,RAYON_NUM_THREADS='1'),stdout=f,stderr=err,check=True)
for v in ['cc','cs','sc','ss']:
 env=dict(os.environ,RAYON_NUM_THREADS='1',DSSIM_MODE=v)
 if host=='linux':env.update(MALLOC_MMAP_THRESHOLD_='67108864',MALLOC_TRIM_THRESHOLD_='2147483647')
 out=root/'results'/f'{host}-{v}.bits'
 with out.open('wb') as f, out.with_suffix('.scores').open('wb') as err:subprocess.run([str(root/'modes/target/release/parity')],env=env,stdout=f,stderr=err,check=True)
 subprocess.run(['cmp',str(base),str(out)],check=True)
 subprocess.run(['cmp',str(basescores),str(out.with_suffix('.scores'))],check=True)
 hasher=hashlib.sha256()
 with out.open('rb') as f:
  for block in iter(lambda:f.read(1024*1024),b''):hasher.update(block)
 digest=hasher.hexdigest()
 out.with_suffix('.sha256').write_text(f'{digest}  {out.name}\n')
 print(host,v,'parity matches',flush=True)
 mem=root/'results'/f'{host}-memory-{v}.csv'
 if host=='linux':
  cmd=['/usr/bin/time','-f','rss_max_kib,%M','-o',str(mem.with_suffix('.rss')), 'taskset','-c','2',str(root/'modes/target/release/memory'),'rgba','2049','1024']
  with mem.open('w') as f:subprocess.run(cmd,env=env,stdout=f,check=True)
 else:
  with mem.open('w') as f,mem.with_suffix('.rss').open('w') as err:subprocess.run(['/usr/bin/time','-l',str(root/'modes/target/release/memory'),'rgba','2049','1024'],env=env,stdout=f,stderr=err,check=True)
 print(mem.read_text(),flush=True)
