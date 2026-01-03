export type IdleCallbackHandle = number

type IdleWindow = Window & {
  requestIdleCallback?: (cb: () => void) => number
  cancelIdleCallback?: (id: number) => void
}

export const scheduleIdle = (callback: () => void): IdleCallbackHandle => {
  if (typeof window === 'undefined') {
    callback()
    return 0
  }

  const idleWindow = window as IdleWindow

  if (idleWindow.requestIdleCallback) {
    return idleWindow.requestIdleCallback(callback)
  }

  return window.setTimeout(callback, 0)
}

export const cancelIdle = (handle: IdleCallbackHandle) => {
  if (typeof window === 'undefined') {
    return
  }

  const idleWindow = window as IdleWindow

  if (idleWindow.cancelIdleCallback) {
    idleWindow.cancelIdleCallback(handle)
    return
  }

  window.clearTimeout(handle)
}
