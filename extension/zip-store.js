// ZIP32 STORE with data descriptors: stream bodies to a file, never buffer an
// entire archive. Central-directory metadata alone stays in memory.
const encoder=new TextEncoder(),table=Uint32Array.from({length:256},(_,n)=>{for(let i=0;i<8;i++)n=n&1?0xedb88320^(n>>>1):n>>>1;return n>>>0});
export function crc32(bytes,crc=0xffffffff){for(const byte of bytes)crc=table[(crc^byte)&255]^(crc>>>8);return crc>>>0}
const record=(size,signature)=>{const bytes=new Uint8Array(size);new DataView(bytes.buffer).setUint32(0,signature,true);return bytes};
export async function writeResourceZip(items,sink,fetcher=fetch,{signal,onProgress=()=>{}}={}){
 let offset=0;const directory=[];const write=async bytes=>{if(offset+bytes.length>0x7fffffff)throw Error('ZIP 超过 2 GiB，请拆分打包或减少所选文件。');await sink.write(bytes);offset+=bytes.length};
 for(let i=0;i<items.length;i++){
  if(signal?.aborted)throw Error('打包已取消。');const item=items[i],name=encoder.encode(item.name);
  if(!name.length||name.length>240||/[\\/\x00-\x1f]/.test(item.name)||item.name==='.'||item.name==='..')throw Error('ZIP 文件名无效。');
  const start=offset,header=record(30,0x04034b50),h=new DataView(header.buffer);h.setUint16(4,20,true);h.setUint16(6,0x0808,true);h.setUint16(26,name.length,true);await write(header);await write(name);
  const response=await fetcher(item.url,{credentials:'include',signal});if(!response.ok||!response.body)throw Error(`第 ${i+1} 项读取失败（${response.status}），未生成不完整 ZIP。`);
  const reader=response.body.getReader();let size=0,crc=0xffffffff;
  try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.length;crc=crc32(value,crc);await write(value);onProgress({index:i+1,total:items.length,bytes:offset})}}catch(error){await reader.cancel().catch(()=>{});throw error}finally{reader.releaseLock()}
  crc=(crc^0xffffffff)>>>0;const descriptor=record(16,0x08074b50),d=new DataView(descriptor.buffer);d.setUint32(4,crc,true);d.setUint32(8,size,true);d.setUint32(12,size,true);await write(descriptor);
  const central=record(46,0x02014b50),c=new DataView(central.buffer);c.setUint16(4,20,true);c.setUint16(6,20,true);c.setUint16(8,0x0808,true);c.setUint32(16,crc,true);c.setUint32(20,size,true);c.setUint32(24,size,true);c.setUint16(28,name.length,true);c.setUint32(42,start,true);directory.push([central,name]);
 }
 const start=offset;for(const parts of directory)for(const part of parts)await write(part);const end=record(22,0x06054b50),e=new DataView(end.buffer);e.setUint16(8,items.length,true);e.setUint16(10,items.length,true);e.setUint32(12,offset-start,true);e.setUint32(16,start,true);await write(end);return {bytes:offset,count:items.length};
}
