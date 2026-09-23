#!/usr/bin/env node
import fs from 'fs';
function cat(parts){ return Buffer.concat(parts); }
function head(major,n){
  if(!Number.isSafeInteger(n)||n<0) throw new Error('bad uint');
  if(n<24) return Buffer.from([(major<<5)|n]);
  if(n<256) return Buffer.from([(major<<5)|24,n]);
  if(n<65536){let b=Buffer.alloc(3);b[0]=(major<<5)|25;b.writeUInt16BE(n,1);return b;}
  if(n<2**32){let b=Buffer.alloc(5);b[0]=(major<<5)|26;b.writeUInt32BE(n,1);return b;}
  let b=Buffer.alloc(9);b[0]=(major<<5)|27;b.writeBigUInt64BE(BigInt(n),1);return b;
}
function enc(x){
  if(x===null) return Buffer.from([0xf6]);
  if(x===false) return Buffer.from([0xf4]);
  if(x===true) return Buffer.from([0xf5]);
  if(typeof x==='number'){
    if(!Number.isInteger(x)) throw new Error('floats forbidden');
    return x>=0?head(0,x):head(1,-1-x);
  }
  if(typeof x==='string'){let b=Buffer.from(x,'utf8');return cat([head(3,b.length),b]);}
  if(Array.isArray(x)) return cat([head(4,x.length),...x.map(enc)]);
  if(typeof x==='object'){
    let keys=Object.keys(x);
    if(keys.length===1 && keys[0]==='$bytes') {let b=Buffer.from(x.$bytes,'hex');return cat([head(2,b.length),b]);}
    let pairs=[];
    if(keys.length===1 && keys[0]==='$map') pairs=x.$map.map(([k,v])=>[enc(k),enc(v)]);
    else pairs=Object.entries(x).map(([k,v])=>[enc(k),enc(v)]);
    pairs.sort((a,b)=>a[0].length-b[0].length || Buffer.compare(a[0],b[0]));
    return cat([head(5,pairs.length),...pairs.flatMap(([k,v])=>[k,v])]);
  }
  throw new Error('unsupported');
}
let fixtures=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));let out={};
for(let f of fixtures){let h=enc(f.value).toString('hex');out[f.id]=h;if(f.expected!==undefined&&h!==f.expected)throw new Error(`${f.id}: expected ${f.expected} got ${h}`);}
process.stdout.write(JSON.stringify(Object.fromEntries(Object.entries(out).sort()))+'\n');
