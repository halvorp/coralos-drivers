import re,sys,subprocess,os
LIN=os.path.expanduser("~/Projects/references/coralos/linux-ref/sound/soc/codecs/")
OUT="/home/peter/Projects/coralos-drivers-wt-avs-wave2/crates/da7219-core/"
os.makedirs(OUT+"src",exist_ok=True); os.makedirs(OUT+"tests",exist_ok=True)

def defs(fname):
    lines=open(LIN+fname).read().split("\n")
    res=[];i=0;in_fields=False
    while i<len(lines):
        l=lines[i]
        if l.startswith("/*") and "Bit Fields" in lines[i+1] if i+1<len(lines) else False: in_fields=True
        m=re.match(r'#define\s+(DA7219_\w+)\s+(.*)$',l)
        if m:
            name,expr=m.group(1),m.group(2).strip()
            start=i
            while expr.endswith("\\"):
                i+=1; expr=expr[:-1].strip()+" "+lines[i].strip()
            expr=re.sub(r'/\*.*?\*/','',expr).strip()
            res.append((name,expr,start+1,in_fields))
        i+=1
    return res

def ev(expr):
    if re.search(r'[A-Za-z_]',re.sub(r'0x[0-9A-Fa-f]+','',expr)): return None
    return eval(expr)

def emit(path,header,items,fname,ty_pick):
    s=["// SPDX-License-Identifier: GPL-2.0-only","//! "+header,"// GENERATED from the Linux header by tools/gen-da7219-consts (values evaluated from the macro text; the test files carry","// the gcc-evaluated expectations, which do not pass through this table).",""]
    for name,expr,ln in items:
        v=ev(expr)
        ty=ty_pick(v)
        s.append(f"pub const {name}: {ty} = {v:#04x}; // {fname}:{ln}")
    open(path,"w").write("\n".join(s)+"\n")

def ty(v): return "u8" if v<=0xFF else "u32"

def dd(L):
    seen=set();o=[]
    for x in L:
        if x[0] in seen: continue
        seen.add(x[0]);o.append(x)
    return o
main=dd(defs("da7219.h")); aad=dd(defs("da7219-aad.h"))
regs=[(n,e,l) for n,e,l,f in main if not f and ev(e) is not None and n!="__DA7219_H"]
fields=[(n,e,l) for n,e,l,f in main if f and ev(e) is not None]
aregs=[(n,e,l) for n,e,l,f in aad if not f and ev(e) is not None]
afields=[(n,e,l) for n,e,l,f in aad if f and ev(e) is not None]
emit(OUT+"src/regs.rs","DA7219 register addresses: da7219.h 'Registers' block and da7219-aad.h (accessory detect) registers. All 8-bit (regmap reg_bits = 8, da7219.c:2440).",regs+aregs,"da7219.h",lambda v:"u8")
# fix file names for aad regs: rewrite citation
t=open(OUT+"src/regs.rs").read().split("\n")
aadnames={n for n,_,_ in aregs}
t=[ (l.replace("// da7219.h:","// da7219-aad.h:") if any(l.startswith(f"pub const {n}:") for n in aadnames) else l) for l in t]
open(OUT+"src/regs.rs","w").write("\n".join(t))
emit(OUT+"src/fields.rs","DA7219 bit fields, masks, shifts and value constants of da7219.h (the 'Bit Fields' and 'General defines' blocks).",fields,"da7219.h",ty)
emit(OUT+"src/aad_fields.rs","DA7219 accessory-detect bit fields and constants of da7219-aad.h.",afields,"da7219-aad.h",ty)
# gcc dump
def cdump(items_main,items_aad):
    src=[]
    for fname,items in (("da7219.h",items_main),("da7219-aad.h",items_aad)):
        for n,e,l in items: src.append(f"#define {n} {e}")
    body="\n".join(src)+"\n#include <stdio.h>\nint main(void){\n"
    for fname,items in (("da7219.h",items_main),("da7219-aad.h",items_aad)):
        for n,e,l in items: body+=f'printf("{fname} %s %llu\\n","{n}",(unsigned long long)({n}));\n'
    body+="return 0;}\n"
    return body
# names are unique across both headers? check
alln=[n for n,_,_ in regs+fields+aregs+afields]
assert len(alln)==len(set(alln)),"dup names"
open("dump.c","w").write(cdump(regs+fields,aregs+afields))
subprocess.check_call(["gcc","-o","dump","dump.c","-w"])
out=subprocess.check_output(["./dump"]).decode().split("\n")
vals={}
for l in out:
    if l: f,n,v=l.split(); vals[n]=(f,int(v))
def mk(testfile,mod,names,doc):
    s=["// SPDX-License-Identifier: GPL-2.0-only",f"//! {doc}","//! Expected values: the Linux macros compiled UNMODIFIED with gcc (printf of each macro), written out literally.",f"use da7219_core::{mod}::*;",""]
    s.append("#[test]")
    s.append(f"fn {mod}_constants_equal_the_values_gcc_computes_from_the_linux_header() {{")
    for n in names:
        s.append(f'    assert_eq!({n}, {vals[n][1]:#x}, "{n}");')
    s.append("}")
    open(OUT+"tests/"+testfile,"w").write("\n".join(s)+"\n")
mk("header_regs.rs","regs",[n for n,_,_ in regs+aregs],"Every register address against the gcc value of the Linux header.")
mk("header_fields.rs","fields",[n for n,_,_ in fields],"Every da7219.h field constant against the gcc value.")
mk("header_aad_fields.rs","aad_fields",[n for n,_,_ in afields],"Every da7219-aad.h field constant against the gcc value.")
print(len(regs),len(aregs),len(fields),len(afields))
