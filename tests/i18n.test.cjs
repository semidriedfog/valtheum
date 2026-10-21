const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const catalog=JSON.parse(fs.readFileSync('static/i18n-catalog.json','utf8'));
function runtime(language){const context=vm.createContext({window:{__valtheumLanguage:language},document:{}});vm.runInContext(fs.readFileSync('static/i18n-catalog.js','utf8'),context);vm.runInContext(fs.readFileSync('static/i18n.js','utf8'),context);return context.window;}
test('all translations preserve interpolation slots and HTML structure',()=>{
 for(const language of ['en','ja'])for(const [key,value]of Object.entries(catalog[language])){
   assert.ok(value.length, key);
   const slots=s=>[...s.matchAll(/\{[^}]*\}/g)].map(m=>m[0]).sort();
   assert.deepEqual(slots(value),slots(key),language+': '+key);
   const tags=s=>s.match(/<\/?(?:div|span|tr|td)\b[^>]*>/g)||[];assert.deepEqual(tags(value),tags(key),language+': '+key);
 }
});
test('translation interpolates once without changing Korean filenames or injecting markup',()=>{
 for(const lang of ['ko','en','ja']){const w=runtime(lang),name='영상_완료_{1}_日本語.zip';const result=w.vt('{0}페이지 · {1}',7,name);assert.ok(result.endsWith(name));assert.ok(result.includes('7'));}
 assert.equal(runtime('invalid').VaultI18n.language,'en');
 assert.equal(runtime(undefined).VaultI18n.language,'en');
});
test('server errors and progress are translated without changing embedded paths',()=>{
 for(const lang of ['en','ja']){const w=runtime(lang),p='D:/영상/日本語.mp4';const error=w.VaultI18n.message('원본 삭제 실패: '+p);assert.ok(error.endsWith(p));assert.ok(!error.startsWith('원본 삭제 실패'));assert.ok(!/[가-힣]/.test(w.VaultI18n.message('해시·암호화')));assert.ok(!/[가-힣]/.test(w.VaultI18n.message('저장 공간이 부족합니다. 필요한 추가 공간: 123 MiB')));}
});

test('static application labels and accessibility text have English and Japanese translations',()=>{
 // Language names and the multilingual language-picker labels stay recognizable in every locale.
 const multilingual=new Set(['한국어','언어 변경 / Language / 言語','Language / 언어 / 言語']);
 const html=fs.readFileSync('static/index.html','utf8').replace(/<!--[\s\S]*?-->|<(script|style|code)\b[^>]*>[\s\S]*?<\/\1>/gi,'');
 const decode=s=>s.replace(/&nbsp;/g,'\u00a0').replace(/&amp;/g,'&').replace(/&lt;/g,'<').replace(/&gt;/g,'>').replace(/&quot;/g,'"');
 const labels=[...html.matchAll(/>([^<]+)</g)].map(m=>decode(m[1]).trim());
 for(const m of html.matchAll(/\b(?:title|aria-label|placeholder|alt)="([^"]+)"/g))labels.push(decode(m[1]));
 for(const label of new Set(labels)){
  if(!/[가-힣]/.test(label)||multilingual.has(label))continue;
  for(const language of ['en','ja'])assert.ok(catalog[language][label],`${language}: ${label}`);
 }
});

test('browser translation bundle matches the source catalog',()=>{
 const w=runtime('en');
 assert.deepEqual(JSON.parse(JSON.stringify(w.VaultMessages)),catalog);
});
