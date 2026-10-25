import {mkdir,readFile,writeFile,access,copyFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import path from 'node:path';
const root=path.resolve(import.meta.dirname,'..'),out=path.join(root,'src-tauri/resources'),temp=path.join(root,'.tools/extractor');
await mkdir(temp,{recursive:true});await mkdir(out,{recursive:true});
let existing=false;try{await Promise.all(['yt-dlp.exe','deno.exe','EXTRACTOR-LICENSE.txt','extractor-build.json'].map(file=>access(path.join(out,file))));existing=true}catch{}
if(existing){const metadata=JSON.parse(await readFile(path.join(out,'extractor-build.json'),'utf8'));for(const entry of metadata.components){if(!['yt-dlp.exe','deno.exe'].includes(entry.binary))throw Error('Unknown extractor component');const hash=createHash('sha256').update(await readFile(path.join(out,entry.binary))).digest('hex');if(hash!==entry.binarySha256)throw Error('Bundled component digest mismatch: '+entry.binary);}console.log('Bundled site extractor digests verified.');process.exit(0)}
const headers={'User-Agent':'SnowDown-Build','Accept':'application/vnd.github+json'};
const get=async url=>{const response=await fetch(url,{headers,signal:AbortSignal.timeout(240000)});if(!response.ok)throw Error(`Official download HTTP ${response.status}`);return response};
let pin;try{pin=JSON.parse(await readFile(path.join(out,'extractor-build.json'),'utf8'))}catch{}
const records=[];
for(const [repo,name,destination]of [['yt-dlp/yt-dlp','yt-dlp.exe','yt-dlp.exe'],['denoland/deno','deno-x86_64-pc-windows-msvc.zip','deno.exe']]){
 const previous=pin?.components.find(x=>x.repository===repo),release=await(await get(`https://api.github.com/repos/${repo}/releases/${previous?'tags/'+previous.version:'latest'}`)).json();
 const asset=release.assets.find(x=>x.name===name);if(!asset?.digest?.startsWith('sha256:'))throw Error('Official release digest is missing: '+name);
 const data=Buffer.from(await(await get(asset.browser_download_url)).arrayBuffer()),hash=createHash('sha256').update(data).digest('hex');
 if('sha256:'+hash!==asset.digest||previous&&previous.sha256!==hash)throw Error('Official release digest differs: '+name);
 if(name.endsWith('.zip')){const archive=path.join(temp,name),expanded=path.join(temp,'deno');await writeFile(archive,data);const child=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command','Expand-Archive -LiteralPath $env:SNOWDOWN_ARCHIVE -DestinationPath $env:SNOWDOWN_EXPANDED -Force'],{env:{...process.env,SNOWDOWN_ARCHIVE:archive,SNOWDOWN_EXPANDED:expanded},stdio:'inherit',windowsHide:true});if(child.status!==0)throw Error('Cannot extract Deno runtime.');await copyFile(path.join(expanded,'deno.exe'),path.join(out,destination))}
 else await writeFile(path.join(out,destination),data);
 records.push({repository:repo,version:release.tag_name,source:asset.browser_download_url,sha256:hash,binary:destination,binarySha256:createHash('sha256').update(await readFile(path.join(out,destination))).digest('hex')});
 console.log('Verified official component '+destination+' '+release.tag_name);
}
const ytdlp=records.find(x=>x.repository==='yt-dlp/yt-dlp'),deno=records.find(x=>x.repository==='denoland/deno');
const licenses=await Promise.all([`https://raw.githubusercontent.com/yt-dlp/yt-dlp/${ytdlp.version}/LICENSE`,`https://raw.githubusercontent.com/yt-dlp/yt-dlp/${ytdlp.version}/THIRD_PARTY_LICENSES.txt`,`https://raw.githubusercontent.com/denoland/deno/${deno.version}/LICENSE.md`].map(async url=>(await get(url)).text()));
await writeFile(path.join(out,'EXTRACTOR-LICENSE.txt'),'yt-dlp Windows combined executable: GPL-3.0-or-later. Source: https://github.com/yt-dlp/yt-dlp\n\n'+licenses.join('\n\n-----\n\n'));
await writeFile(path.join(out,'extractor-build.json'),JSON.stringify({components:records},null,2)+'\n');
