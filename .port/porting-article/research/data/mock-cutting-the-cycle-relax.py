import json, math
P="/Users/gael/projects/rustty/.port/journey-article/research/data/mock-cutting-the-cycle-embed.json"
d=json.load(open(P))
N=d["crates"]["nodes"]; E=[e for e in d["crates"]["edges"] if e[2]!="dev"]
H=560.0
idx={n[0]:i for i,n in enumerate(N)}
dep=[n[2] for n in N]; r=[2.6+16*math.sqrt(n[3]/174265) for n in N]
y=[n[5]*H for n in N]
nb=[[] for _ in N]
for a,b,k in E: nb[idx[a]].append(idx[b]); nb[idx[b]].append(idx[a])
def lab(i): return len(N[i][0].replace("rustty-",""))*5.0+r[i]+5   # label width to the right
for it in range(700):
    # attraction toward neighbours
    ny=y[:]
    for i in range(len(N)):
        if nb[i]:
            m=sum(y[j] for j in nb[i])/len(nb[i]); ny[i]=y[i]+(0.03 if it<250 else 0.0)*(m-y[i])
    y=ny
    # label/node collisions: any pair whose x-range overlaps (incl. label) and |dy| too small
    for i in range(len(N)):
        for j in range(i+1,len(N)):
            xi=34+dep[i]*600/18; xj=34+dep[j]*600/18
            ai=(xi-r[i],xi+lab(i)); aj=(xj-r[j],xj+lab(j))
            if ai[1]<aj[0] or aj[1]<ai[0]: continue
            need=max(r[i],6)+max(r[j],6)+(4 if dep[i]!=dep[j] else 6)
            dy=y[j]-y[i]
            if abs(dy)<need:
                push=(need-abs(dy))/2*(1 if dy>=0 else -1)
                if dy==0: push=need/2*(1 if i%2 else -1)
                y[i]-=push; y[j]+=push
    if it<250:
        lo=min(y); hi=max(y); y=[(v-lo)/(hi-lo)*H for v in y]
    else:
        y=[min(H,max(0,v)) for v in y]
for i,n in enumerate(N): n[5]=round(y[i]/H,4)
# report residual collisions
bad=0
for i in range(len(N)):
    for j in range(i+1,len(N)):
        xi=34+dep[i]*600/18; xj=34+dep[j]*600/18
        ai=(xi-r[i],xi+lab(i)); aj=(xj-r[j],xj+lab(j))
        if ai[1]<aj[0] or aj[1]<ai[0]: continue
        if abs(y[i]-y[j])<max(r[i],6)+max(r[j],6)+2: bad+=1; print("collide",N[i][0],N[j][0],round(y[i]),round(y[j]))
print("bad",bad)
json.dump(d,open(P,"w"),separators=(",",":"))
