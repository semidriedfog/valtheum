// Select dependencies of the shipped Windows binaries, not every Cargo.lock entry.
const cp=require('node:child_process');
const path=require('node:path');
const target='x86_64-pc-windows-msvc';
function appMetadata(){
 return JSON.parse(cp.execFileSync('cargo',['metadata','--locked','--offline','--format-version','1',
  '--filter-platform',target,'--manifest-path','Cargo.toml'],
  {cwd:path.resolve(__dirname,'../..'),encoding:'utf8',maxBuffer:32*1024*1024}));
}
function selectPackages(metadata){
 const packages=new Map(metadata.packages.map(p=>[p.id,p]));
 const nodes=new Map(metadata.resolve.nodes.map(n=>[n.id,n]));
 const selected=new Map(),visited=new Set();
 function visit(id){
  if(visited.has(id))return;visited.add(id);
  const p=packages.get(id),node=nodes.get(id);
  if(!p||!node)throw Error('Incomplete Cargo dependency graph: '+id);
  const generated=p.targets.some(t=>t.kind.includes('proc-macro'));
  if(p.source)selected.set(p.name+'-'+p.version,{...p,scope:generated?'generated':'app'});
  // Keep notices for macros that emit app code, but not their host-only helpers.
  if(generated)return;
  for(const dep of node.deps)if(dep.dep_kinds.some(k=>k.kind===null))visit(dep.pkg);
 }
 if(!metadata.resolve.root)throw Error('App Cargo root is missing');
 visit(metadata.resolve.root);
 return selected;
}
module.exports={target,appMetadata,selectPackages};
