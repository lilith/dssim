from pathlib import Path
import collections,csv,re,statistics,sys
root=Path(sys.argv[1]);out=Path(sys.argv[2]);out.mkdir(parents=True,exist_ok=True)
rows=[]
for host in ['linux','mac']:
 for p in sorted((root/(host+'-results')).glob('*.csv')):
  m=re.fullmatch(r'(\d+)x(\d+)-t(\d+)-(compare|candidate|pair)-(current|pr197|modes)-n(\d+)-r([123])',p.stem)
  if not m:continue
  records=list(csv.DictReader(p.open()))
  assert len(records)==1,p
  rows.append(dict(host=host,width=m[1],height=m[2],workers=m[3],scenario=m[4],lineage=m[5],chunk=m[6],round=m[7],**records[0]))
with (out/'results.csv').open('w') as f:
 w=csv.DictWriter(f,fieldnames=rows[0],lineterminator='\n');w.writeheader();w.writerows(rows)
groups=collections.defaultdict(list);bits=collections.defaultdict(set)
keys=['host','width','height','workers','scenario','lineage','chunk']
for r in rows:
 groups[tuple(r[k] for k in keys)].append(float(r['median_ns'])/1e6)
 bits[tuple(r[k] for k in ['host','width','height'])].add(r['score_bits'])
assert all(len(x)==1 for x in bits.values()),'Score mismatch'
with (out/'summary.csv').open('w') as f:
 w=csv.writer(f,lineterminator='\n');w.writerow(keys+['runs','median_ms','min_ms','max_ms'])
 for k,v in sorted(groups.items()):
  assert len(v)==3,(k,len(v))
  w.writerow([*k,len(v),statistics.median(v),min(v),max(v)])
print(len(rows),'accepted runs, all scores agree per host/shape')
