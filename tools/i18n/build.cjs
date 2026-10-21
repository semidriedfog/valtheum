const fs=require('fs');const keys=JSON.parse(fs.readFileSync('tools/i18n/source-strings.json','utf8'));
const messages={en:{},ja:{}};const rows=fs.readFileSync('tools/i18n/translations.tsv','utf8').replace(/^\uFEFF/,'').trimEnd().split(/\r?\n/);const ids=new Set();
for(const row of rows){const [id,en,ja]=row.split('\t');if(!en||!ja||ids.has(Number(id)))throw Error('Invalid translation '+id);ids.add(Number(id));messages.en[keys[id]]=en.replaceAll('\\n','\n');messages.ja[keys[id]]=ja.replaceAll('\\n','\n');}
if(ids.size!==keys.length)throw Error('Missing translations');
if(fs.existsSync('tools/i18n/extra.json')){const extra=JSON.parse(fs.readFileSync('tools/i18n/extra.json','utf8'));for(const [k,[en,ja]]of Object.entries(extra)){messages.en[k]=en;messages.ja[k]=ja;}}
fs.writeFileSync('static/i18n-catalog.json',JSON.stringify(messages,null,2)+'\n');
fs.writeFileSync('static/i18n-catalog.js','window.VaultMessages = '+JSON.stringify(messages).replaceAll('<','\\u003c')+';\n');
console.log(Object.keys(messages.en).length+' translated messages per language');
