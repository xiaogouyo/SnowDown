// Temporary visual bridge; the real forms retain their state and receive focus
// only after the bridge and the outgoing visual copy have been removed.
const duration = 820
const easing = 'cubic-bezier(.25,.1,.25,1)'
const formSelector = '.link-form, .minimal-input-bar'
type Box = { left: number; top: number; width: number; height: number }
function box(element: Element): Box {
  const { left, top, width, height } = element.getBoundingClientRect()
  return { left, top, width, height }
}
function visual(element: Element) {
  const style = getComputedStyle(element)
  return { borderRadius: style.borderRadius, backgroundColor: style.backgroundColor, borderColor: style.borderColor, boxShadow: style.boxShadow, color: style.color, fontSize: style.fontSize, fontWeight: style.fontWeight, gap: style.gap }
}
function sample(root: HTMLElement) {
  const form = root.querySelector<HTMLElement>(formSelector)
  const input = form?.querySelector('input')
  const action = form?.querySelector<HTMLElement>('.analyze-button, .minimal-parse')
  const glyph = action?.querySelector('svg')
  const link = form?.querySelector('.link-icon, .minimal-link-icon')
  if (!form || !input || !action || !glyph || !link || form.getBoundingClientRect().width < 1) return null
  const extras = [...form.querySelectorAll<HTMLElement>('.paste-button, .minimal-clear, .minimal-platform')].map(node => ({ copy: node.cloneNode(true) as HTMLElement, box: box(node) }))
  return { form, box: box(form), style: visual(form), input, inputBox: box(input), inputStyle: visual(input), placeholderColor: getComputedStyle(input, '::placeholder').color, action, actionBox: box(action), actionStyle: visual(action), glyphBox: box(glyph), linkBox: box(link), linkColor: getComputedStyle(link).color, linkSvg: link.innerHTML, extras }
}
type Sample = NonNullable<ReturnType<typeof sample>>
function relative(child: Box, parent: Box) {
  // Absolute children start inside the bridge's one-pixel border.
  return { left: `${child.left - parent.left - 1}px`, top: `${child.top - parent.top - 1}px`, width: `${child.width}px`, height: `${child.height}px` }
}
function shape(sample: Sample) {
  return { left: `${sample.box.left}px`, top: `${sample.box.top}px`, width: `${sample.box.width}px`, height: `${sample.box.height}px`, borderRadius: sample.style.borderRadius, backgroundColor: sample.style.backgroundColor, borderColor: sample.style.borderColor, boxShadow: sample.style.boxShadow }
}
function element(tag: string, className: string, parent: HTMLElement) {
  const node = document.createElement(tag)
  node.className = className
  parent.append(node)
  return node
}
function svg(parent: HTMLElement, className: string, markup: string) {
  const node = document.createElementNS('http://www.w3.org/2000/svg', 'svg')
  for (const [key, value] of Object.entries({ class: className, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round', 'stroke-linejoin': 'round' })) node.setAttribute(key, value)
  node.innerHTML = markup // Trusted local icon paths only; user text uses textContent.
  parent.append(node)
  return node
}

export function prepareModeMorph(root: HTMLElement | null, event?: MouseEvent) {
  if (!root || typeof root.animate !== 'function' || window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return null
  const initial = sample(root)
  if (!initial) return null
  const from: Sample = initial
  const minimal = root.classList.contains('standard-mode')
  const anchor = event?.currentTarget instanceof Element ? box(event.currentTarget) : box(root.querySelector('.brand') ?? root)
  const x = Math.max(0, Math.min(innerWidth, event && event.detail > 0 ? event.clientX : anchor.left + anchor.width / 2))
  const y = Math.max(0, Math.min(innerHeight, event && event.detail > 0 ? event.clientY : anchor.top + anchor.height / 2))
  const radius = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y)) + 2
  const capture = element('div', 'mode-switch-capture', document.body)
  capture.setAttribute('aria-hidden', 'true')
  capture.inert = true
  const clone = root.cloneNode(true) as HTMLElement
  capture.append(clone)
  const sourceBrand = root.querySelector<HTMLElement>('.brand')
  const fixedBrand = sourceBrand?.cloneNode(true) as HTMLElement | undefined
  if (sourceBrand && fixedBrand) {
    const bounds = box(sourceBrand)
    fixedBrand.classList.add('mode-morph-brand')
    fixedBrand.setAttribute('aria-hidden', 'true')
    fixedBrand.inert = true
    Object.assign(fixedBrand.style, { position: 'fixed', left: `${bounds.left}px`, top: `${bounds.top}px`, width: `${bounds.width}px`, height: `${bounds.height}px`, textAlign: getComputedStyle(sourceBrand).textAlign })
    document.body.append(fixedBrand)
    const copiedBrand = clone.querySelector<HTMLElement>('.brand')
    if (copiedBrand) copiedBrand.style.visibility = 'hidden'
  }
  const originals = root.querySelectorAll('.workspace, .minimal-body')
  clone.querySelectorAll('.workspace, .minimal-body').forEach((node, index) => { node.scrollTop = originals[index]?.scrollTop ?? 0 })
  clone.querySelector<HTMLElement>(formSelector)!.style.visibility = 'hidden'
  const bridge = element('div', 'mode-morph-bar', document.body)
  bridge.setAttribute('aria-hidden', 'true')
  bridge.inert = true
  Object.assign(bridge.style, shape(from))
  const animations: Animation[] = []
  let target: HTMLElement | undefined
  let targetForm: HTMLElement | null = null
  let oldVisibility = ''
  let targetBrand: HTMLElement | null = null
  let brandVisibility = ''
  let frame = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  let resolve: (() => void) | undefined
  let stopped = false
  const done = new Promise<void>(complete => { resolve = complete })
  function cancel() {
    if (stopped) return
    stopped = true
    cancelAnimationFrame(frame)
    clearTimeout(timer)
    window.removeEventListener('resize', cancel)
    animations.forEach(animation => animation.cancel())
    target?.classList.remove('mode-reveal-layer')
    if (targetForm) targetForm.style.visibility = oldVisibility
    if (targetBrand) targetBrand.style.visibility = brandVisibility
    capture.remove(); bridge.remove(); fixedBrand?.remove(); resolve?.()
  }
  function animate(node: Element, keyframes: Keyframe[]) {
    const animation = node.animate(keyframes, { duration, easing, fill: 'both' })
    animations.push(animation)
    return animation
  }
  function extras(sample: Sample, entering: boolean, other: Sample) {
    for (const original of sample.extras) {
      const extra = original.copy
      extra.removeAttribute('id'); extra.classList.add('mode-morph-extra'); bridge.append(extra)
      const hidden = { ...relative(other.actionBox, other.box), opacity: 0, transform: 'scale(.55)' }
      const visible = { ...relative(original.box, sample.box), opacity: 1, transform: 'scale(1)' }
      animate(extra, entering ? [hidden, { ...hidden, offset: .3 }, visible] : [visible, { ...hidden, offset: .65 }, { ...hidden, offset: 1 }])
    }
    if (!sample.action.classList.contains('analyze-button')) return
    const label = element('span', 'mode-morph-label', bridge)
    label.textContent = sample.action.textContent
    const left = sample.glyphBox.left + sample.glyphBox.width + (parseFloat(sample.actionStyle.gap) || 8) - sample.box.left - 1
    const visible = { left: `${left}px`, top: `${sample.actionBox.top - sample.box.top - 1}px`, height: `${sample.actionBox.height}px`, color: sample.actionStyle.color, opacity: 1 }
    Object.assign(label.style, { fontSize: sample.actionStyle.fontSize, fontWeight: sample.actionStyle.fontWeight })
    const hidden = { ...relative(other.actionBox, other.box), color: other.actionStyle.color, opacity: 0 }
    animate(label, entering ? [hidden, { ...hidden, offset: .6 }, visible] : [visible, { ...hidden, offset: .28 }, { ...hidden, offset: 1 }])
  }
  async function play(next: HTMLElement | null) {
    if (stopped) return done
    target = next ?? undefined
    const to = target && sample(target)
    if (!target || !to) { cancel(); return done }
    try {
      targetForm = to.form
      oldVisibility = targetForm.style.visibility
      targetForm.style.visibility = 'hidden'
      targetBrand = target.querySelector<HTMLElement>('.brand')
      brandVisibility = targetBrand?.style.visibility ?? ''
      if (targetBrand && fixedBrand) targetBrand.style.visibility = 'hidden'
      target.classList.add('mode-reveal-layer')
      animate(target, [{ clipPath: `circle(0px at ${x}px ${y}px)` }, { clipPath: `circle(${radius}px at ${x}px ${y}px)` }])
      const movement = animate(bridge, [shape(from), shape(to)])
      const link = svg(bridge, 'mode-morph-link', from.linkSvg)
      animate(link, [{ ...relative(from.linkBox, from.box), color: from.linkColor }, { ...relative(to.linkBox, to.box), color: to.linkColor }])
      for (const [sample, entering] of [[from, false], [to, true]] as const) {
        const text = element('span', 'mode-morph-text', bridge)
        const placeholder = !sample.input.value
        text.textContent = sample.input.value || sample.input.placeholder
        const frames = [from, to].map(item => ({ ...relative(item.inputBox, item.box), fontSize: item.inputStyle.fontSize, color: placeholder ? item.placeholderColor : item.inputStyle.color }))
        if (!entering && from.input.value && from.input.value === to.input.value) animate(text, frames)
        else if (entering && from.input.value && from.input.value === to.input.value) text.remove()
        else animate(text, entering
          ? [{ ...frames[0], opacity: 0 }, { ...frames[1], opacity: 0, offset: .38 }, { ...frames[1], opacity: 1 }]
          : [{ ...frames[0], opacity: 1 }, { ...frames[1], opacity: 0, offset: .35 }, { ...frames[1], opacity: 0 }])
      }
      const action = element('span', 'mode-morph-action', bridge)
      animate(action, [from, to].map(item => ({ ...relative(item.actionBox, item.box), backgroundColor: item.actionStyle.backgroundColor, borderRadius: item.actionStyle.borderRadius, boxShadow: item.actionStyle.boxShadow })))
      const glyph = svg(bridge, 'mode-morph-glyph', '<path class="mode-icon-shaft"/><path class="mode-icon-head"/><path class="mode-icon-tray" d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>')
      animate(glyph, [from, to].map(item => ({ ...relative(item.glyphBox, item.box), color: item.actionStyle.color })))
      const shaft = glyph.querySelector('.mode-icon-shaft')!
      const head = glyph.querySelector('.mode-icon-head')!
      const tray = glyph.querySelector('.mode-icon-tray')!
      const spinner = from.action.querySelector('.spin')?.cloneNode(true) as SVGElement | undefined
      if (spinner) { glyph.replaceChildren(...spinner.childNodes); glyph.classList.add('spin') }
      function draw() {
        if (stopped) return
        const progress = Number(movement.effect?.getComputedTiming().progress ?? 0)
        const p = minimal ? progress : 1 - progress
        const mix = (a: number, b: number) => +(a + (b - a) * p).toFixed(3)
        shaft.setAttribute('d', `M${mix(12, 7)} ${mix(3, 17)}L${mix(12, 17)} ${mix(15, 7)}`)
        head.setAttribute('d', `M7 ${mix(10, 7)}L${mix(12, 17)} ${mix(15, 7)}L17 ${mix(10, 17)}`)
        tray.setAttribute('opacity', String(1 - p))
        frame = requestAnimationFrame(draw)
      }
      draw(); extras(from, false, to); extras(to, true, from)
      window.addEventListener('resize', cancel)
      timer = setTimeout(cancel, duration + 180)
      void Promise.allSettled(animations.map(animation => animation.finished)).then(cancel)
    } catch { cancel() }
    return done
  }
  return { play, cancel }
}
