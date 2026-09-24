from pathlib import Path
import io,json,shutil,subprocess,sys,tarfile
here=Path(__file__).resolve().parent
repo=subprocess.check_output(['git','rev-parse','--show-toplevel'],cwd=here,text=True).strip()
root=Path(sys.argv[1]).resolve();root.mkdir(parents=True,exist_ok=False)
revisions={r['variant'].split('-')[0]:r['revision'] for r in json.loads((here/'variants.json').read_text())}
for name in ['build.py','run.py','validate.py','summarize.py']:
 shutil.copy2(here/name,root/name)
for lineage,revision in revisions.items():
 source=root/('source-'+lineage);source.mkdir()
 archive=subprocess.check_output(['git','archive','--format=tar',revision],cwd=repo)
 with tarfile.open(fileobj=io.BytesIO(archive)) as tar:tar.extractall(source,filter='data')
 shutil.copytree(here/lineage,root/lineage)
 name='dssim.rs' if lineage=='pr197' else 'dssim_rows.rs'
 shutil.copy2(source/'dssim-core/src'/name,root/(lineage+'-original.rs'))
 print(lineage,revision)
