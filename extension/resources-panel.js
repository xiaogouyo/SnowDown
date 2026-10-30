import {resourceView,resourceFormat,resourceFilename} from './resource-tools.js';
export function resourcesPanel(send,message){
 const $=id=>document.getElementById(id);let items=[],selected=new Set();
 const filters=()=>({kind:$('kind').value,format:$('format').value,minWidth:+$('width').value,maxWidth:+$('widthMax').value||Infinity,minHeight:+$('height').value,maxHeight:+$('heightMax').value||Infinity,query:$('query').value,sort:$('resourceSort').value});
 const visible=()=>resourceView(items,filters());
 function bounds(){for(const axis of ['width','height']){const range=$(axis+'Range'),upper=$(axis+'Max'),max=Math.max(0,...items.map(x=>x[axis]||0));if(max>0){const wasFull=!upper.value||+upper.value===+range.max;range.max=String(max);if(wasFull)upper.value=String(max)}}}

 for(const axis of ['width','height']){const range=$(axis+'Range');range.oninput=()=>{$(axis).value=range.value;render()};$(axis).addEventListener('input',()=>{range.value=$(axis).value})}
 function count(){const rows=visible(),n=rows.filter(x=>selected.has(x.url)).length;$('count').textContent=`${n} / ${rows.length}`;$('all').checked=!!rows.length&&n===rows.length;$('all').indeterminate=n>0&&n<rows.length;for(const id of ['downloadSelected','packSelected'])$(id).disabled=n===0;}
 function render(){
  const rows=visible();$('list').replaceChildren();$('list').classList.toggle('large-preview',$('previewSize').value==='large');
  for(const item of rows){
   const card=document.createElement('article');card.className='resource-card';card.classList.toggle('selected',selected.has(item.url));const choice=document.createElement('input');choice.type='checkbox';choice.setAttribute('aria-label','选择 '+(item.name||resourceFormat(item)));choice.checked=selected.has(item.url);choice.onchange=()=>{choice.checked?selected.add(item.url):selected.delete(item.url);card.classList.toggle('selected',choice.checked);count()};card.append(choice);
   const preview=document.createElement('div');preview.className='resource-preview';
   const detail=document.createElement('span');
   const label=()=>{detail.textContent=`${item.width&&item.height?item.width+'×'+item.height+' · ':''}${resourceFormat(item).toUpperCase()}`};label();
   if(item.kind==='image'){const image=document.createElement('img');image.loading='lazy';image.src=item.url;image.alt=item.name||'图片预览';image.referrerPolicy='no-referrer';image.onload=()=>{item.width=image.naturalWidth||item.width;item.height=image.naturalHeight||item.height;bounds();label()};preview.append(image)}else{const icon=document.createElement('span');icon.className='resource-glyph';icon.textContent={video:'▷',audio:'♫',document:'▤',manifest:'≡'}[item.kind]||'▧';preview.append(icon);const name=document.createElement('small');name.textContent=item.name||new URL(item.url).pathname.split('/').pop()||'资源';preview.append(name)}
   preview.onclick=()=>{choice.checked=!choice.checked;choice.onchange()};card.append(preview);
   const bar=document.createElement('div');bar.className='resource-meta';bar.append(detail);
   for(const [text,title,fn]of [['↗','打开资源',()=>chrome.tabs.create({url:item.url})],['↓','下载此资源',()=>send('download',{resource:{...item,name:resourceFilename(item,0,$('rename').value)}})]]){const b=document.createElement('button');b.textContent=text;b.title=title;b.setAttribute('aria-label',title);b.onclick=async()=>{b.disabled=true;try{await fn()}catch(e){message(e.message)}finally{b.disabled=false}};bar.append(b)}card.append(bar);$('list').append(card);
  }
  if(!rows.length){const empty=document.createElement('p');empty.className='resource-empty';empty.textContent='没有符合条件的资源，试试更换分类或筛选条件。';$('list').append(empty)}count();
 }
 for(const id of ['kind','format','width','widthMax','height','heightMax','query','resourceSort','previewSize'])$(id).addEventListener('input',()=>{document.querySelectorAll('[data-resource-kind]').forEach(b=>b.classList.toggle('active',b.dataset.resourceKind===$('kind').value));render()});
 document.querySelectorAll('[data-resource-kind]').forEach(b=>b.onclick=()=>{$('kind').value=b.dataset.resourceKind;$('format').value='';if(['document','audio','manifest'].includes(b.dataset.resourceKind)){for(const axis of ['width','height']){$(axis).value='0';$(axis+'Range').value='0'}}render();document.querySelectorAll('[data-resource-kind]').forEach(x=>x.classList.toggle('active',x===b))});
 $('all').onchange=()=>{for(const item of visible())$('all').checked?selected.add(item.url):selected.delete(item.url);render()};
 $('invert').onclick=()=>{for(const item of visible())selected.has(item.url)?selected.delete(item.url):selected.add(item.url);render()};
 const chosen=()=>visible().filter(item=>selected.has(item.url));
 for(const [id,pack]of [['downloadSelected',false],['packSelected',true]])$(id).onclick=async()=>{
  const rows=chosen();if(!rows.length)return;$(id).disabled=true;try{
   if(pack){const result=await send('archive',{items:rows,name:$('archiveName').value||'网页资源',template:$('rename').value});message(result.message)}
   else{let done=0,failed=0;for(let i=0;i<rows.length;i++){try{await send('download',{resource:{...rows[i],name:resourceFilename(rows[i],i,$('rename').value)}});done++}catch{failed++}}message(`已提交 ${done} 项${failed?'，'+failed+' 项失败，请单独重试。':'下载。'}`)}
  }catch(e){message(e.message)}finally{count()}
 };
 $('exportLinks').onclick=async()=>{const rows=chosen().length?chosen():visible();const blob=URL.createObjectURL(new Blob([rows.map(x=>x.url).join('\n')],{type:'text/plain;charset=utf-8'}));try{await chrome.downloads.download({url:blob,filename:'SnowDown/资源链接.txt',saveAs:false})}finally{setTimeout(()=>URL.revokeObjectURL(blob),60000)}};
 return {setItems(value){items=value;selected.clear();bounds();const formats=[...new Set(items.map(resourceFormat))].sort();const previous=$('format').value;$('format').replaceChildren(new Option('全部格式',''),...formats.map(f=>new Option(f.toUpperCase()+' '+items.filter(x=>resourceFormat(x)===f).length,f)));$('format').value=formats.includes(previous)?previous:'';render()},render};
}
