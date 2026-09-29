const $ = s => document.querySelector(s);
const iconMarkup = id => `<svg class="ui-icon" aria-hidden="true" focusable="false"><use href="#om-icon-${id}"></use></svg>`;
const root = document.documentElement;
const chatsView = $('#chatsView');
const conversationView = $('#conversationView');
const emptyView = $('#emptyView');
const composer = $('#composer');
const rootOmni = $('#omnibarRoot');
const rootShellRow = $('#rootShellRow');
const convOmni = $('#conversationOmni');
const results = $('#omniResults');
const tabs = $('#tabs');
const backBtn = $('#backBtn');
const title = $('#screenTitle');
const subtitle = $('#screenSubtitle');
const toast = $('#toast');
let currentPerson = 'Maria Santos';
let preservedDraft = '';
let activeResult = 0;
let currentItems = [];
let activeOmniInput = null;
let settingsInvoker = null;

const candidates = [
  {icon:'account',title:'Maria Santos',detail:'Open conversation',kind:'Person',q:'maria santos message chat person',action:'open',target:'Maria Santos'},
  {icon:'call',title:'Call Maria Santos',detail:'Explicit call action',kind:'Action',q:'call maria santos',action:'demo',target:'Call Maria Santos'},
  {icon:'search',title:'Search messages with Maria',detail:'Search this conversation',kind:'Action',q:'search maria messages',action:'demo',target:'Search messages with Maria'},
  {icon:'invite',title:'Invite someone',detail:'Link or QR',kind:'Action',q:'invite add person qr link',action:'empty'},
  {icon:'qr-scan',title:'Scan QR code',detail:'Add or open an invite',kind:'Action',q:'scan qr code',action:'demo',target:'Scan QR'},
  {icon:'settings',title:'Appearance',detail:'OwnMessenger settings',kind:'Setting',q:'settings appearance theme dark light',action:'settings'},
  {icon:'settings',title:'Dark appearance',detail:'Switch this prototype',kind:'Setting',q:'dark mode appearance',action:'dark'},
  {icon:'chats',title:'Family',detail:'Open group conversation',kind:'Conversation',q:'family group chat',action:'open',target:'Family'},
  {icon:'account',title:'Ren',detail:'Open conversation',kind:'Person',q:'ren message chat person',action:'open',target:'Ren'}
];

function showToast(message) {
  toast.textContent = message;
  toast.classList.remove('hidden');
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(()=>toast.classList.add('hidden'),1800);
}

function clearOmniState({hide=true}={}) {
  [$('#rootOmniInput'), $('#conversationOmniInput')].forEach(input => {
    input.setAttribute('aria-expanded','false');
    input.removeAttribute('aria-activedescendant');
  });
  if (hide) results.classList.add('hidden');
  activeOmniInput = null;
}

function setView(view, person) {
  chatsView.classList.toggle('hidden', view !== 'chats');
  conversationView.classList.toggle('hidden', view !== 'conversation');
  emptyView.classList.toggle('hidden', view !== 'empty');
  composer.classList.toggle('hidden', view !== 'conversation');
  rootShellRow.classList.toggle('hidden', view === 'conversation');
  convOmni.classList.add('hidden');
  clearOmniState();
  backBtn.classList.toggle('hidden', view === 'chats');
  if (view === 'conversation') {
    currentPerson = person || currentPerson;
    title.textContent = currentPerson;
    subtitle.textContent = 'Private conversation · fixture';
    $('#composerInput').placeholder = `Message ${currentPerson.split(' ')[0]}…`;
  } else if (view === 'empty') {
    title.textContent = 'OM'; subtitle.textContent = 'Private by default';
  } else {
    title.textContent = 'Chats'; subtitle.textContent = 'Validation fixture';
  }
}

function queryItems(query='') {
  const q = query.trim().toLowerCase();
  // Stable fixture ordering: explicit text filters candidates; no stochastic reordering.
  return candidates.filter(x => !q || x.q.includes(q) || x.title.toLowerCase().includes(q)).slice(0,6);
}

function syncActiveDescendant() {
  if (!activeOmniInput || results.classList.contains('hidden') || !currentItems.length) {
    activeOmniInput?.removeAttribute('aria-activedescendant');
    return;
  }
  activeOmniInput.setAttribute('aria-activedescendant', `omni-option-${activeResult}`);
}

function renderResults(input, query='') {
  activeOmniInput = input;
  currentItems = queryItems(query);
  activeResult = 0;
  results.innerHTML = currentItems.map((x,i)=>`<div id="omni-option-${i}" class="omni-result" role="option" aria-selected="${i===0}" data-index="${i}"><span class="result-icon" aria-hidden="true">${iconMarkup(x.icon)}</span><span class="result-copy"><strong>${x.title}</strong><small>${x.detail}</small></span><span class="result-kind">${x.kind}</span></div>`).join('');
  results.classList.toggle('hidden', currentItems.length===0);
  [$('#rootOmniInput'), $('#conversationOmniInput')].forEach(other => {
    const isActive = other === input && currentItems.length > 0;
    other.setAttribute('aria-expanded', String(isActive));
    if (!isActive) other.removeAttribute('aria-activedescendant');
  });
  syncActiveDescendant();
  results.querySelectorAll('.omni-result').forEach(option => {
    option.addEventListener('pointerdown', e => e.preventDefault()); // keep DOM focus in combobox
    option.addEventListener('click',()=>activateResult(Number(option.dataset.index)));
  });
}

function paintSelection() {
  results.querySelectorAll('.omni-result').forEach((el,i)=>el.setAttribute('aria-selected',String(i===activeResult)));
  syncActiveDescendant();
  const selected=results.querySelector(`[data-index="${activeResult}"]`);
  selected?.scrollIntoView({block:'nearest'});
}

function moveSelection(delta) {
  if (!currentItems.length) return;
  activeResult = (activeResult + delta + currentItems.length) % currentItems.length;
  paintSelection();
}

function activateResult(index=activeResult) {
  const item=currentItems[index]; if(!item) return;
  clearOmniState();
  if(item.action==='open') setView('conversation',item.target);
  else if(item.action==='empty') setView('empty');
  else if(item.action==='settings') {
    settingsInvoker = activeOmniInput || document.activeElement;
    $('#settingsPanel').classList.remove('hidden');
    $('#settingsBtn').setAttribute('aria-expanded','true');
    $('#themeSelect').focus();
  }
  else if(item.action==='dark') { root.dataset.mode='dark'; $('#modeSelect').value='dark'; showToast('Dark appearance selected'); }
  else { showToast(`${item.target} — fixture action`); }
}

function openConversationOmni() {
  preservedDraft = $('#composerInput').value;
  composer.classList.add('hidden');
  convOmni.classList.remove('hidden');
  $('#conversationOmniInput').value = '';
  $('#conversationOmniInput').focus();
  renderResults($('#conversationOmniInput'), '');
}
function closeConversationOmni() {
  convOmni.classList.add('hidden');
  clearOmniState();
  composer.classList.remove('hidden');
  $('#composerInput').value = preservedDraft;
  $('#composerInput').focus();
}

function bindOmniInput(input) {
  input.addEventListener('focus',e=>renderResults(input,e.target.value));
  input.addEventListener('input',e=>renderResults(input,e.target.value));
  input.addEventListener('keydown',e=>{
    if(e.key==='ArrowDown'){e.preventDefault();moveSelection(1)}
    else if(e.key==='ArrowUp'){e.preventDefault();moveSelection(-1)}
    else if(e.key==='Enter' && !results.classList.contains('hidden')){e.preventDefault();activateResult()}
    else if(e.key==='Escape' && !results.classList.contains('hidden')){e.preventDefault();clearOmniState()}
  });
}

document.querySelectorAll('.chat-row').forEach(btn=>btn.addEventListener('click',()=>setView('conversation',btn.dataset.person)));
backBtn.addEventListener('click',()=>setView('chats'));
$('#conversationOmniTrigger').addEventListener('click',openConversationOmni);
$('#closeOmniBtn').addEventListener('click',closeConversationOmni);
bindOmniInput($('#rootOmniInput'));
bindOmniInput($('#conversationOmniInput'));
$('#rootOmniTrigger').addEventListener('click',()=>{$('#rootOmniInput').focus();renderResults($('#rootOmniInput'),$('#rootOmniInput').value)});
$('#sendBtn').addEventListener('click',()=>{
  const input=$('#composerInput'); const text=input.value.trim(); if(!text) return;
  const bubble=document.createElement('div'); bubble.className='message outgoing'; bubble.textContent=text;
  const state=document.createElement('span'); state.className='message-state'; state.innerHTML=iconMarkup('sent')+'<span>Sent</span>'; bubble.appendChild(state);
  $('#messages').appendChild(bubble); input.value=''; preservedDraft='';
  bubble.scrollIntoView({behavior:root.dataset.motion==='reduce'?'auto':'smooth',block:'nearest'});
  input.focus();
});
document.querySelectorAll('.retry-button').forEach(btn=>btn.addEventListener('click',()=>showToast('Retry queued — fixture')));

$('#settingsBtn').addEventListener('click',()=>{
  const panel=$('#settingsPanel'); const opening=panel.classList.contains('hidden');
  settingsInvoker = $('#settingsBtn');
  panel.classList.toggle('hidden'); $('#settingsBtn').setAttribute('aria-expanded',String(opening));
  if(opening) $('#themeSelect').focus();
});
$('#emptyBtn').addEventListener('click',()=>setView('empty'));
$('#themeSelect').addEventListener('change',e=>root.dataset.theme=e.target.value);
$('#modeSelect').addEventListener('change',e=>root.dataset.mode=e.target.value);
$('#densitySelect').addEventListener('change',e=>root.dataset.density=e.target.value);
$('#personalitySelect').addEventListener('change',e=>root.dataset.personality=e.target.value);
$('#textScaleSelect').addEventListener('change',e=>root.dataset.textScale=e.target.value);
$('#contrastSelect').addEventListener('change',e=>root.dataset.contrast=e.target.value);
$('#motionSelect').addEventListener('change',e=>root.dataset.motion=e.target.value);
$('#visualCheckSelect').addEventListener('change',e=>root.dataset.check=e.target.value);
$('#tabsToggle').addEventListener('change',e=>tabs.classList.toggle('hidden',!e.target.checked));

document.querySelectorAll('.tab').forEach(btn=>btn.addEventListener('click',()=>{
  document.querySelectorAll('.tab').forEach(x=>{x.classList.toggle('active',x===btn);x.toggleAttribute('aria-current',x===btn)});
  if(btn.dataset.tab==='chats') setView('chats'); else showToast(`${btn.textContent.trim()} view — fixture only`);
}));

document.addEventListener('keydown',e=>{
  if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='k'){
    e.preventDefault();
    if(!composer.classList.contains('hidden')) openConversationOmni();
    else{$('#rootOmniInput').focus();renderResults($('#rootOmniInput'),$('#rootOmniInput').value)}
  }
  if(e.key==='Escape'){
    if(!convOmni.classList.contains('hidden')) closeConversationOmni();
    else if(!results.classList.contains('hidden')) clearOmniState();
    else if(!$('#settingsPanel').classList.contains('hidden')){
      $('#settingsPanel').classList.add('hidden');
      $('#settingsBtn').setAttribute('aria-expanded','false');
      const target=settingsInvoker;
      if(target && target.isConnected && target.getClientRects().length) target.focus();
      else if(!composer.classList.contains('hidden')) $('#composerInput').focus();
      else $('#rootOmniInput').focus();
    }
  }
});

if('serviceWorker' in navigator && location.protocol!=='file:') navigator.serviceWorker.register('./sw.js').catch(()=>{});
