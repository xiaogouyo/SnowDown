// Keep closed buttons in the card's own layout/scroll/transform tree. No
// viewport coordinates or scroll callbacks are needed to move a button.
const mounts=new WeakMap();
export function cardMount(target){
 let node=/^(IMG|VIDEO|PICTURE|SOURCE)$/.test(target.tagName)?target.parentElement:target;
 for(let depth=0;node&&depth<12;node=node.parentElement,depth++){
  if(/^(IMG|VIDEO|PICTURE|SOURCE|SVG)$/.test(node.tagName))continue;
  const display=getComputedStyle(node).display;
  if(display!=='contents'&&display!=='inline'&&display!=='none')return node;
 }
 return null;
}
function acquire(mount){
 const saved=mounts.get(mount);if(saved){saved.count++;return;}
 const properties=[];
 for(const [name,value,needed]of [['position','relative',getComputedStyle(mount).position==='static'],['isolation','isolate',getComputedStyle(mount).isolation!=='isolate']])if(needed){
  const before=mount.style.getPropertyValue(name),priority=mount.style.getPropertyPriority(name);mount.style.setProperty(name,value);properties.push({name,value,before,priority});
 }
 mounts.set(mount,{count:1,properties});
}
function release(mount){
 const saved=mounts.get(mount);if(!saved||--saved.count)return;
 for(const {name,value,before,priority}of saved.properties)if(mount.style.getPropertyValue(name)===value&&!mount.style.getPropertyPriority(name)){
  if(before)mount.style.setProperty(name,before,priority);else mount.style.removeProperty(name);
 }
 mounts.delete(mount);
}
export function bindCardAnchor(host,target){
 const mount=cardMount(target);
 if(!mount)return null;acquire(mount);mount.append(host);
 let released=false;
 function update(){
  if(released||!target.isConnected||!mount.isConnected)return false;
  const frame=mount.getBoundingClientRect(),box=target.getBoundingClientRect();
  const sx=frame.width/(mount.offsetWidth||frame.width||1),sy=frame.height/(mount.offsetHeight||frame.height||1);
  const cropped=/(hidden|clip)/.test(getComputedStyle(mount).overflowX+' '+getComputedStyle(mount).overflowY);
  const corner=target===mount||(Math.abs(box.left-frame.left)<2&&Math.abs(box.right-frame.right)<2&&Math.abs(box.top-frame.top)<2)||cropped&&box.left<=frame.left+2&&box.right>=frame.right-2&&box.top<=frame.top+2;
  host.style.position='absolute';host.style.margin='0';host.style.bottom='auto';host.style.visibility='visible';
  host.dataset.placement='top-left';host.style.transform='none';host.style.right='auto';
  host.style.left=corner?'6px':`${(box.left-frame.left)/sx+mount.scrollLeft-mount.clientLeft+6}px`;
  host.style.top=corner?'6px':`${(box.top-frame.top)/sy+mount.scrollTop-mount.clientTop+6}px`;
  return true;
 }
 update();
 return {mount,target,update,release(){if(released)return;released=true;release(mount);}};
}
