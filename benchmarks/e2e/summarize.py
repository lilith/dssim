"""Validate score bits and summarize full lifecycles, including phase costs."""
from pathlib import Path
from collections import defaultdict
import csv, json, statistics, sys
root=Path(sys.argv[1])
rows=[]; groups=defaultdict(list); scores=defaultdict(set)
phases=['reference_prep','candidate_prep','compare','drop','reference_once']
for host in ['linux','mac']:
    folder=root/('results-'+host)
    for p in sorted(folder.glob('*.csv')):
        if '.rejected' in p.name: continue
        shape,workers,scenario,version,mode,round_no=p.stem.split('-')
        r=next(csv.DictReader(p.open()))
        row=dict(host=host,size=shape,workers=int(workers[1:]),scenario=scenario,version=version,
                 mode=mode,round=int(round_no[1:]),median_ms=float(r['median_ns'])/1e6,
                 **{x+'_ms':float(r[x+'_ns'])/1e6 for x in phases},
                 **{k:v for k,v in r.items() if k=='score_bits' or k.startswith('batch_')})
        phase_sum=sum(row[x+'_ms'] for x in ['reference_prep','candidate_prep','compare','drop'])
        assert phase_sum <= row['median_ms'] + 1e-6, (p, 'phases exceed total')
        assert row['median_ms']-phase_sum < row['median_ms']*.01, (p, 'timer overhead exceeds 1%')
        rows.append(row)
        groups[(host,shape,row['workers'],scenario,version,mode)].append(row)
        scores[host,shape].add(row['score_bits'])
assert len(rows)==216, f'Expected 216 accepted observations, found {len(rows)}'
assert all(len(g)==3 for g in groups.values()), 'Missing or duplicate rounds'
assert all(len(s)==1 for s in scores.values()), f'Score mismatch: {scores}'
with (root/'results.csv').open('w') as f:
    w=csv.DictWriter(f,fieldnames=list(rows[0]),lineterminator='\n');w.writeheader();w.writerows(rows)
summary={}
for k,group in groups.items():
    g=sorted(group,key=lambda r:r['median_ms'])
    summary[k]=dict(g[1],min_ms=g[0]['median_ms'],max_ms=g[2]['median_ms'],
                   reference_once_ms=statistics.median(r['reference_once_ms'] for r in g))
with (root/'summary.csv').open('w') as f:
    w=csv.DictWriter(f,fieldnames=[x for x in next(iter(summary.values())) if not x.startswith('batch_')],lineterminator='\n')
    w.writeheader();w.writerows({k:v for k,v in r.items() if not k.startswith('batch_')} for _,r in sorted(summary.items()))
lines=[]
variants={
    'pair':[('main','cc','Upstream main'),('options','ss','Fresh pair streaming'),('options','cc','Fresh pair cached')],
    'candidate':[('main','cc','Upstream main'),('options','cs','Cached ref / streaming candidate'),('options','cc','Cached both / traditional')],
}
def table(title,scenario):
    vs=variants[scenario]
    lines.extend([f'## {title}','', '| Host | Size | Workers | '+' | '.join(label for _,_,label in vs)+' |',
                  '| --- | --- | ---: | '+' | '.join('---:' for _ in vs)+' |'])
    for host in ['linux','mac']:
        for shape in ['512x512','3840x2160']:
            for workers in [1,6]:
                vals=[summary[host,shape,workers,scenario,v,m]['median_ms'] for v,m,_ in vs]
                lines.append(f'| {host} | {shape} | {workers} | '+' | '.join(f'{v:.2f}' for v in vals)+' |')
    lines.append('')
table('Fresh pair: both preparations + comparison + destruction, ms','pair')
table('Reused reference: candidate preparation + comparison + destruction, ms','candidate')
lines.extend(['## Preparation and full-operation breakdown, ms','',
              'Reference-once is a separately measured setup cost for the reused-reference workload. All other phases come from the same timed operation as the total. Add reference-once once per reference, not once per candidate.', '',
              '| Host | Size | Workers | Workload / policy | Reference once | Reference prep per operation | Candidate prep | Compare | Drop | Total per operation |',
              '| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |'])
for host in ['linux','mac']:
    for shape in ['512x512','3840x2160']:
        for workers in [1,6]:
            for scenario in ['pair','candidate']:
                for v,m,label in variants[scenario]:
                    r=summary[host,shape,workers,scenario,v,m]
                    vals=[r[x+'_ms'] for x in phases]+[r['median_ms']]
                    # Display setup before the phases that are inside the operation.
                    vals=[r['reference_once_ms'],r['reference_prep_ms'],r['candidate_prep_ms'],r['compare_ms'],r['drop_ms'],r['median_ms']]
                    lines.append(f'| {host} | {shape} | {workers} | {scenario}: {label} | '+' | '.join(f'{v:.3f}' for v in vals)+' |')
lines.extend(['','## Attribution controls: fresh pair, caching both, ms','',
              '| Host | Size | Workers | Upstream main | #197 dispatch | #198 fusion | #200 cached |',
              '| --- | --- | ---: | ---: | ---: | ---: | ---: |'])
for host in ['linux','mac']:
    for shape in ['512x512','3840x2160']:
        for workers in [1,6]:
            vals=[summary[host,shape,workers,'pair',v,'cc']['median_ms'] for v in ['main','dispatch','fusion','options']]
            lines.append(f'| {host} | {shape} | {workers} | '+' | '.join(f'{v:.2f}' for v in vals)+' |')
lines.extend(['','## Across-process ranges','','Each cell is median (minimum–maximum) of three process medians, ms.', '',
              '| Host | Size | Workers | Workload | Revision / policy | Time |',
              '| --- | --- | ---: | --- | --- | ---: |'])
for k,r in sorted(summary.items()):
    host,shape,workers,scenario,version,mode=k
    lines.append(f'| {host} | {shape} | {workers} | {scenario} | {version} / {mode} | {r["median_ms"]:.3f} ({r["min_ms"]:.3f}–{r["max_ms"]:.3f}) |')
lines.extend(['','## Timing validation','',f'{len(rows)} accepted observations; all seven batches and their phases are in `results.csv`. The four candidate score bit patterns agree across every revision, policy, worker count, and workload within each host/size:', ''])
for (host,shape),s in sorted(scores.items()):lines.append(f'- {host} {shape}: `{next(iter(s))}`')
for host in ['linux','mac']:
    records=[json.loads(s) for s in (root/('results-'+host)/'runs.jsonl').read_text().splitlines()]
    lines.append(f'- {host}: {sum(not r["rejected"] for r in records)} accepted, {sum(r["rejected"] for r in records)} rejected for competing CPU load.')
(root/'tables.md').write_text('\n'.join(lines)+'\n')
print('\n'.join(lines[:27]))
