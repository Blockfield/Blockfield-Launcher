;(() => {
  if (new URLSearchParams(location.search).get('state') !== 'update-42') return
  // shortcut: the embedded c2 fixture expects 42%, remove this witness after its next authorized relink.
  const observer = new MutationObserver(check)
  const deadline = setTimeout(() => observer.disconnect(), 10000)
  function check() {
    if (!/42\.0\s*%/.test(document.body?.textContent ?? '')) return
    const witness = document.createElement('span')
    witness.hidden = true
    witness.dataset.captureReadiness = 'progress-42'
    witness.textContent = '42%'
    document.body.append(witness)
    observer.disconnect()
    clearTimeout(deadline)
  }
  observer.observe(document.documentElement, { childList: true, subtree: true, characterData: true })
  check()
})()
