app.initDropdown = el => {
  el.isDropdownOpen = () => el.classList.contains('open')

  let hoverTimeoutHandle
  app.whenHoveringOver(el, h => {
    if (el.isDropdownOpen()) return
    el.toggleDropdown(true)
  })

  el.toggleDropdown = (state = !el.isDropdownOpen()) => {
    if (!state) {
      el.classList.remove('open')
      el.clickawayHandle.off()
      if (hoverTimeoutHandle) hoverTimeoutHandle.off()
      app.openDropdown = null
      return el
    }
    if (app.openDropdown) app.openDropdown.toggleDropdown(false)
    app.openDropdown = el
    el.classList.add('open')
    el.clickawayHandle.on()
    hoverTimeoutHandle = app.whenNotHoveringOver(el, h => {
      clearTimeout(el.closeTimeout)
      el.closeTimeout = setTimeout(() => {
        if (el.isDropdownOpen()) {
          el.toggleDropdown(false)
          h.off()
        }
      }, 1500)
      app.whenHoveringOver(el, h2 => {
        clearTimeout(el.closeTimeout)
        h2.off()
        h.on()
      })
    })

    return el
  }

  el.clickawayHandle = domlib.on.click(document.body, e => {
    if (e.target != el && !el.contains(e.target)) {
      el.toggleDropdown(false)
    }
  }).off()
}

app.NotHoveringOverHandlers = []
app.HoveringOverHandlers = []
app.whenNotHoveringOver = (el, h) => {
  h.el = el
  const leaveHandle = domlib.on.pointerleave(el, e => {
    if (!h.notified) {
      h.notified = true
      h(h)
    }
  }).off()
  h.off = () => {
    if (h.isOn) {
      h.isOn = false
      app.NotHoveringOverHandlers.splice(h.i, 1)
      leaveHandle.off()
    }
    return h
  }
  h.on = () => {
    if (!h.isOn) {
      h.isOn = true
      h.i = app.NotHoveringOverHandlers.push(h) - 1
      leaveHandle.on()
    }
    return h
  }
  return h.on()
}
app.whenHoveringOver = (el, h) => {
  h.el = el
  h.off = () => {
    app.HoveringOverHandlers.splice(h.i, 1)
    h.isOn = false
    return h
  }
  h.on = () => {
     if (!h.isOn) {
       h.isOn = true
       h.i = app.HoveringOverHandlers.push(h) - 1
     }
     return h
  }
  return h.on()
}
app.isHoveringOver = el => app.hoveringOn.includes(el)
app.hoverElementChanged = path => {
  for (const h of app.NotHoveringOverHandlers) {
    if (app.isHoveringOver(h.el)) {
      h.notified = false
    } else if (!h.notified) {
      h.notified = true
      h(h)
    }
  }
  for (const h of app.HoveringOverHandlers) {
    if (!app.isHoveringOver(h.el)) {
      h.notified = false
    } else if (!h.notified) {
      h.notified = true
      h(h)
    }
  }
}

domlib.run(() => {
  app.compareHoverPath = e => {
    if (app.lastHoveringOn) {
      if (e.path[0] != app.lastHoveringOn[0]) {
        app.lastHoveringOn = app.hoveringOn
        app.hoverElementChanged(app.hoveringOn = e.path)
      }
      return
    }
    app.hoverElementChanged(app.lastHoveringOn = app.hoveringOn = e.path)
  }
  //document.body.addEventListener('pointermove', app.compareHoverPath, {passive: true})
  document.body.addEventListener('pointerover', app.compareHoverPath, {passive: true})
  document.body.addEventListener('pointerenter', app.compareHoverPath, {passive: true})

  ;(app.dropdowns = [...document.querySelectorAll('.dropdown')]).forEach(app.initDropdown)
})
