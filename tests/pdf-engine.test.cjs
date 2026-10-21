// Exercise the bundled PDF engine itself with a generated high-resolution image.
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs');
const {deflateSync}=require('node:zlib'),{pathToFileURL}=require('node:url'),path=require('node:path');
const http=require('node:http');
// The bundled browser build uses APIs newer than this test runner's Node runtime.
Math.sumPrecise ??= values => [...values].reduce((sum,value)=>sum+value,0);
Map.prototype.getOrInsertComputed ??= function(k,fn){if(!this.has(k))this.set(k,fn(k));return this.get(k);};
Map.prototype.getOrInsert ??= function(k,v){if(!this.has(k))this.set(k,v);return this.get(k);};
Uint8Array.prototype.toHex ??= function(){return Buffer.from(this).toString('hex');};
globalThis.DOMMatrix ??= class {constructor(){this.a=this.d=1;this.b=this.c=this.e=this.f=0;}};

test('official Liberation release fonts load and produce PDF text in all four styles',async()=>{
 const requested=[];
 const fonts=['LiberationSans-Regular.ttf','LiberationSans-Bold.ttf','LiberationSans-Italic.ttf','LiberationSans-BoldItalic.ttf'];
 const server=http.createServer((req,res)=>{
  const name=req.url.slice(1);
  if(!fonts.includes(name)){res.writeHead(404);return res.end();}
  requested.push(name);res.end(fs.readFileSync(path.join(__dirname,'../static/vendor/pdfjs/standard_fonts',name)));
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 let task;
 try {
  const pdf=await import(pathToFileURL(path.join(__dirname,'../static/vendor/pdfjs/pdf.mjs')));
  pdf.GlobalWorkerOptions.workerSrc=pathToFileURL(path.join(__dirname,'../static/vendor/pdfjs/pdf.worker.mjs')).href;
  const text='License font test 0123456789';
  const commands=['Helvetica','Helvetica-Bold','Helvetica-Oblique','Helvetica-BoldOblique'].map((_,i)=>`BT /F${i} 18 Tf 20 ${700-i*40} Td (${text}) Tj ET`).join('\n');
  const bodies=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Count 1 /Kids [3 0 R] >>',
   '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F0 5 0 R /F1 6 0 R /F2 7 0 R /F3 8 0 R >> >> /Contents 4 0 R >>',
   `<< /Length ${Buffer.byteLength(commands)} >>\nstream\n${commands}\nendstream`,
   ...['Helvetica','Helvetica-Bold','Helvetica-Oblique','Helvetica-BoldOblique'].map(name=>`<< /Type /Font /Subtype /Type1 /BaseFont /${name} /Encoding /WinAnsiEncoding >>`)];
  let data='%PDF-1.7\n';const offsets=[0];
  bodies.forEach((body,i)=>{offsets.push(Buffer.byteLength(data));data+=`${i+1} 0 obj\n${body}\nendobj\n`;});
  const xref=Buffer.byteLength(data);data+=`xref\n0 9\n0000000000 65535 f \n${offsets.slice(1).map(n=>String(n).padStart(10,'0')+' 00000 n \n').join('')}trailer\n<< /Size 9 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  task=pdf.getDocument({data:new Uint8Array(Buffer.from(data)),standardFontDataUrl:`http://127.0.0.1:${server.address().port}/`,useWorkerFetch:true,useSystemFonts:false,disableFontFace:true,fontExtraProperties:true,isEvalSupported:false,stopAtErrors:true});
  const page=await (await task.promise).getPage(1),ops=await page.getOperatorList();
  assert.deepEqual([...new Set(requested)].sort(),[...fonts].sort());
  const names=ops.fnArray.flatMap((op,i)=>op===pdf.OPS.setFont?[ops.argsArray[i][0]]:[]);
  assert.equal(new Set(names).size,4);
  for(const name of names){const font=page.commonObjs.get(name);assert.ok(font.data?.length>1000,'font parsed without fallback: '+name);}
  const content=await page.getTextContent();assert.equal(content.items.filter(item=>item.str===text).length,4);
 } finally {if(task)await task.destroy();await new Promise(resolve=>server.close(resolve));}
});

function imagePdf(width,height){
 const chunks=[Buffer.from('%PDF-1.7\n')],offsets=[0];let offset=chunks[0].length;
 function object(id,body){offsets[id]=offset;const bytes=Buffer.concat([Buffer.from(id+' 0 obj\n'),Buffer.isBuffer(body)?body:Buffer.from(body),Buffer.from('\nendobj\n')]);chunks.push(bytes);offset+=bytes.length;}
 object(1,'<< /Type /Catalog /Pages 2 0 R >>');object(2,'<< /Type /Pages /Count 1 /Kids [3 0 R] >>');
 object(3,'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /XObject << /Im0 5 0 R >> >> /Contents 4 0 R >>');
 const commands=Buffer.from('q 600 0 0 800 0 0 cm /Im0 Do Q');
 object(4,Buffer.concat([Buffer.from('<< /Length '+commands.length+' >>\nstream\n'),commands,Buffer.from('\nendstream')]));
 const image=deflateSync(Buffer.alloc(width*height*3,180));
 object(5,Buffer.concat([Buffer.from('<< /Type /XObject /Subtype /Image /Width '+width+' /Height '+height+' /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length '+image.length+' >>\nstream\n'),image,Buffer.from('\nendstream')]));
 chunks.push(Buffer.from('xref\n0 6\n0000000000 65535 f \n'+offsets.slice(1).map(n=>String(n).padStart(10,'0')+' 00000 n \n').join('')+'trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n'+offset+'\n%%EOF\n'));
 return Buffer.concat(chunks);
}

test('bundled PDF engine preserves images above the old 16MP limit and rejects oversize images explicitly',async()=>{
 const viewer=fs.readFileSync(path.join(__dirname,'../static/pdf-viewer.js'),'utf8');
 const [,size,strict,area]=viewer.match(/maxImageSize:(\d+), stopAtErrors:(true|false), canvasMaxAreaInBytes:(\d+)/);
 const options={maxImageSize:Number(size),stopAtErrors:strict==='true',canvasMaxAreaInBytes:Number(area),isImageDecoderSupported:false,isEvalSupported:false};
 const pdf=await import(pathToFileURL(path.join(__dirname,'../static/vendor/pdfjs/pdf.mjs')));
 pdf.GlobalWorkerOptions.workerSrc=pathToFileURL(path.join(__dirname,'../static/vendor/pdfjs/pdf.worker.mjs')).href;
 const data=imagePdf(4096,4097);
 const task=pdf.getDocument({...options,data:new Uint8Array(data)});
 try {
  const doc=await task.promise;const ops=await (await doc.getPage(1)).getOperatorList();
  assert.equal(ops.fnArray.filter(n=>n===pdf.OPS.paintImageXObject||n===pdf.OPS.paintInlineImageXObject).length,1);
 } finally {await task.destroy();}
 // Reuse the valid image with a lower bound to exercise the same oversized-image path without allocating a giant fixture.
 const oversized=pdf.getDocument({...options,maxImageSize:4096*4097-1,data:new Uint8Array(data)});
 try {
  const doc=await oversized.promise;
  const page=await doc.getPage(1);
  for(let attempt=0;attempt<3;attempt++) {
   await assert.rejects(page.getOperatorList(),/Image exceeded maximum allowed size/);
  }
 } finally {await oversized.destroy();}
 const renderFailure=pdf.getDocument({...options,maxImageSize:4096*4097-1,data:new Uint8Array(data)});
 try {
  const page=await (await renderFailure.promise).getPage(1);
  // No pixels can be painted before this image-size error; a minimal canvas is enough.
  const canvas={width:600,height:800};
  const context=new Proxy({canvas,getTransform:()=>new DOMMatrix(),getLineDash:()=>[]},{get:(target,key)=>key in target?target[key]:()=>{}});
  canvas.getContext=()=>context;
  // A failed prefetch and later foreground render use the same PDFPageProxy.
  // Retrying (including after a zoom change) must not accept a partial cached list.
  for(const scale of [1,1,2]) {
   await assert.rejects(page.render({canvasContext:context,viewport:page.getViewport({scale})}).promise,/Image exceeded maximum allowed size/);
  }
 } finally {await renderFailure.destroy();}
});
