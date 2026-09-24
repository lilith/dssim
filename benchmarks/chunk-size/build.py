from pathlib import Path
import hashlib,json,os,shutil,subprocess
root=Path(__file__).resolve().parent
(root/'bin').mkdir(exist_ok=True);(root/'variants').mkdir(exist_ok=True)
plans={'current': [1024,2048,4096,8192], 'pr197':[1024,4096], 'modes':[1024,4096]}
identities={'current':'1b891ad2597d70f21d90ffb874bac53fd00a37c9','pr197':'eb41ae307bfda358e41c022df2e2956fe5fd868c','modes':'f300435f330056a7707e39aa53bad014c34373e8'}
env=dict(os.environ);env.pop('RUSTFLAGS',None);env.pop('CARGO_ENCODED_RUSTFLAGS',None)
records=[]
for lineage,chunks in plans.items():
 source=root/('source-'+lineage)
 path=source/'dssim-core/src'/('dssim.rs' if lineage=='pr197' else 'dssim_rows.rs')
 original=(root/(lineage+'-original.rs')).read_text()
 for n in chunks:
  tag=f'{lineage}-{n}'
  if lineage=='pr197':
   assert original.count('const SSIM3_CHUNK: usize = 1 << 12;')==1
   changed=original.replace('const SSIM3_CHUNK: usize = 1 << 12;',f'const SSIM3_CHUNK: usize = {n};')
  else:
   assert original.count('4096')==4
   changed=original.replace('4096',str(n))
  path.write_text(changed)
  (root/'variants'/(tag+'.rs')).write_text(changed)
  cmd=['cargo','+1.90.0','build','--release','--locked','--manifest-path',str(root/lineage/'Cargo.toml'),'--bins']
  if lineage=='modes':cmd+=['--features','modes']
  with (root/(tag+'-build.log')).open('w') as f:subprocess.run(cmd,env=env,stdout=f,stderr=f,check=True)
  for name in ['review-bench','parity']:shutil.copy2(root/lineage/'target/release'/name,root/'bin'/(tag+('-parity' if name=='parity' else '')))
  records.append(dict(variant=tag,revision=identities[lineage],chunk_pixels=n,source_sha256=hashlib.sha256(changed.encode()).hexdigest()))
  print('BUILT',tag,flush=True)
 path.write_text(original)
(root/'variants.json').write_text(json.dumps(records,indent=2)+'\n')
