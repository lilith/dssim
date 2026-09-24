from pathlib import Path
import filecmp,hashlib,json,os,platform,subprocess
root=Path(__file__).resolve().parent
host='mac' if platform.system()=='Darwin' else 'linux';out=root/(host+'-validation');out.mkdir(exist_ok=True)
# All variants use cached pairs. Compare entire pixel/map corpus, including odd
# dimensions and padding, against #197 at the original 4096-pixel chunk size.
tags=['pr197-4096','pr197-1024','current-1024','current-2048','current-4096','current-8192','modes-1024','modes-4096']
records=[]
for tag in tags:
 bits=out/(tag+'.bits');scores=out/(tag+'.scores')
 with bits.open('wb') as f,scores.open('wb') as err:
  subprocess.run([str(root/'bin'/(tag+'-parity'))],env=dict(os.environ,RAYON_NUM_THREADS='1',DSSIM_MODE='cc'),stdout=f,stderr=err,check=True)
 if tag!=tags[0]:
  assert filecmp.cmp(bits,out/(tags[0]+'.bits'),shallow=False),tag
  assert filecmp.cmp(scores,out/(tags[0]+'.scores'),shallow=False),tag
 with bits.open('rb') as f:digest=hashlib.file_digest(f,'sha256').hexdigest()
 records.append(dict(variant=tag,bytes=bits.stat().st_size,sha256=digest))
 print('PARITY',host,tag,flush=True)
(out/'parity.json').write_text(json.dumps(records,indent=2)+'\n')
