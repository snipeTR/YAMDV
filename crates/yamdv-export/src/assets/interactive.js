(function(){"use strict";
var d=document,m=d.querySelector("main");if(!m)return;
var th=d.getElementById("th"),pl=d.getElementById("pl"),pd=d.getElementById("pd");
if(th&&pl&&pd)th.addEventListener("click",function(){var l=pl.media==="all";pl.media=l?"not all":"all";pd.media=l?"all":"not all";});
var hs=[],kids=m.children;
function lv(e){var t=e.tagName;return t.length===2&&t[0]==="H"&&t[1]>="1"&&t[1]<="6"?+t[1]:0;}
function sync(){var stop=9;for(var i=0;i<kids.length;i++){var e=kids[i],l=lv(e);if(l&&l<=stop)stop=9;e.hidden=stop<9&&!(l&&l<=stop);if(l&&l<=3&&e.getAttribute("data-folded")==="1"&&stop===9)stop=l;}}
for(var i=0;i<kids.length;i++){var e=kids[i];if(lv(e)&&lv(e)<=3){var b=d.createElement("button");b.type="button";b.className="fold";b.setAttribute("aria-expanded","true");b.setAttribute("aria-label","Collapse section");b.textContent="\u25BE";b.addEventListener("click",(function(h,b){return function(ev){ev.preventDefault();var f=h.getAttribute("data-folded")==="1";h.setAttribute("data-folded",f?"0":"1");b.setAttribute("aria-expanded",f?"true":"false");b.textContent=f?"\u25BE":"\u25B8";sync();};})(e,b));e.insertBefore(b,e.firstChild);hs.push(e);}}
function unfold(){for(var i=0;i<hs.length;i++){hs[i].setAttribute("data-folded","0");var b=hs[i].firstChild;if(b&&b.className==="fold"){b.setAttribute("aria-expanded","true");b.textContent="\u25BE";}}sync();}
d.addEventListener("click",function(ev){var a=ev.target.closest?ev.target.closest("a[href^='#']"):null;if(a)unfold();});
var q=d.getElementById("q"),qn=d.getElementById("qn"),hits=[],cur=-1,last="";
function clear(){for(var i=0;i<hits.length;i++){var k=hits[i],p=k.parentNode;if(p){p.replaceChild(d.createTextNode(k.textContent),k);p.normalize();}}hits=[];cur=-1;}
function skip(n){for(var e=n.parentNode;e&&e!==m;e=e.parentNode){var t=e.tagName;if(t==="SCRIPT"||t==="STYLE"||t==="svg"||t==="BUTTON")return true;}return false;}
function find(s){clear();last=s;if(!s)return;var w=d.createTreeWalker(m,NodeFilter.SHOW_TEXT,null),ns=[],n,ls=s.toLowerCase();while((n=w.nextNode()))if(!skip(n))ns.push(n);
for(var j=0;j<ns.length&&hits.length<2000;j++){n=ns[j];var at;while(n&&(at=n.data.toLowerCase().indexOf(ls))>=0&&hits.length<2000){var r=n.splitText(at),rest=r.splitText(s.length),k=d.createElement("mark");k.className="hit";r.parentNode.replaceChild(k,r);k.appendChild(r);hits.push(k);n=rest;}}}
function show(){if(qn)qn.textContent=hits.length?(cur+1)+"/"+hits.length:(last?"0":"");}
function go(dir){if(!hits.length){show();return;}if(cur>=0)hits[cur].className="hit";cur=(cur+dir+hits.length)%hits.length;var k=hits[cur];k.className="hit cur";if(k.closest&&k.closest("[hidden]"))unfold();k.scrollIntoView({block:"center"});show();}
if(q){q.addEventListener("keydown",function(ev){if(ev.key==="Enter"){ev.preventDefault();if(q.value!==last){find(q.value);cur=-1;}go(ev.shiftKey?-1:1);}else if(ev.key==="Escape"){q.value="";clear();last="";show();}});
q.addEventListener("input",function(){if(!q.value){clear();last="";show();}});
d.addEventListener("keydown",function(ev){if((ev.ctrlKey||ev.metaKey)&&!ev.altKey&&(ev.key==="f"||ev.key==="F")&&d.activeElement!==q){ev.preventDefault();q.focus();q.select();}});}
})();
