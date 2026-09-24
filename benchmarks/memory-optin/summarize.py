"""Aggregate accepted timing CSVs; preserve batch samples and check score parity."""
from pathlib import Path
import collections,csv,re,statistics,sys
root=Path(sys.argv[1]);out=Path(sys.argv[2]);out.mkdir(parents=True,exist_ok=True)
rows=[]
for dataset,host,folder in [
 ('main','mac','results'),('main','linux','linux-results'),
 ('post_creation','mac','api-results'),('post_creation','linux','linux-api-results'),
 ('construction','mac','construction-results'),('construction','linux','linux-construction-results'),
 ('focused','linux','linux-focused-results'),('focused','mac','mac-focused-results'),
]:
 for p in sorted((root/folder).glob('*.csv')):
  m=re.fullmatch(r'(\d+)x(\d+)-t(\d+)-(linear|rgba8)-(compare|candidate|pair)-(\w+)-r([1-7])',p.stem)
  if not m:continue # rejected .attemptN.csv files are not accepted samples
  entries=list(csv.DictReader(p.open()))
  if not entries:raise ValueError('Incomplete file: '+str(p))
  rows.append(dict(dataset=dataset,host=host,width=m[1],height=m[2],workers=m[3],format=m[4],scenario=m[5],variant=m[6],round=m[7],**entries[0]))
with (out/'results.csv').open('w') as f:
 w=csv.DictWriter(f,fieldnames=rows[0],lineterminator='\n');w.writeheader();w.writerows(rows)
groups=collections.defaultdict(list);bits=collections.defaultdict(set)
keys=['dataset','host','width','height','workers','format','scenario','variant']
for r in rows:
 groups[tuple(r[k] for k in keys)].append(float(r['median_ns'])/1e6)
 bits[tuple(r[k] for k in ['host','width','height','format'])].add(r['score_bits'])
assert all(len(x)==1 for x in bits.values()),'Score mismatch'
with (out/'summary.csv').open('w') as f:
 w=csv.writer(f,lineterminator='\n');w.writerow(keys+['runs','median_ms','min_run_ms','max_run_ms'])
 for k,v in sorted(groups.items()):
  assert len(v)==(7 if k[0]=='focused' else 3),(k,len(v))
  w.writerow([*k,len(v),statistics.median(v),min(v),max(v)])
print(len(rows),'accepted observations; scores agree for every host/shape/format')
