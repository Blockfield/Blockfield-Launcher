/** Play pressed while the pack was stale routes to Updates; the game must start
 *  on its own once the sync finishes. A manual visit to Updates never arms the
 *  intent, so plain updates stay on the done screen. */
export function shouldAutoLaunchAfterSync({
  phase,
  playTriggered,
  launching,
  gameBusy,
}: {
  phase: string
  playTriggered: boolean
  launching: boolean
  gameBusy: boolean
}): boolean {
  if (!playTriggered || launching || gameBusy) return false
  return phase === 'complete' || phase === 'uptodate'
}
