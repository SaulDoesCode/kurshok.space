import domlib from '/js/domlib.min.js'

const app = {}

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
  document.body.addEventListener('pointerover', app.compareHoverPath, {
    passive: true
  })
  document.body.addEventListener('pointerenter', app.compareHoverPath, {
    passive: true
  })

  ;
  (app.dropdowns = [...document.querySelectorAll('.dropdown')]).forEach(app.initDropdown)
})


app.writs = []
{
const jsonPost = (url, body) => fetch(url, {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json'
  },
  body: JSON.stringify(body)
})
const jsonPut = (url, body) => fetch(url, {
  method: 'PUT',
  headers: {
    'Content-Type': 'application/json'
  },
  body: JSON.stringify(body)
})

const writQuery = (query = {}) => new Promise((resolve, reject) => {
  if (isNaN(query.page)) query.page = 1
  if (!query.kind) query.kind = 'post'
  jsonPost('/writs', query).then(res => res.json().then(resolve))
})

const pushWrit = (title, raw_content, tags = ['musings'], opts = {}) => new Promise((resolve, reject) => {
  const raw_writ = {
    title,
    raw_content: raw_content.trim(),
    tags,
    kind: 'post',
    public: true,
    viewable_by: []
  }
  Object.assign(raw_writ, opts)
  jsonPut('/writ', raw_writ)
    .then(res => res.json())
    .then(data => data.ok ? resolve(data.data) : reject(data))
})

const wq = (page = 1) => fetch('/writs', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json'
  },
  body: `{"page":${page},"kind":"post"}`
})

const pw = () => pushWrit(
  'Second Writ',
`
** ain't this some shiiit.. **    
first writ of many that came before
`.trim(),
['testing', 'development']
)

const putComment = ({
  parent_id = 'post:0:1',
  writ_id = 'post:0:1',
  raw_content = 'This is a comment, and it\'s even unique nogal, see: ' + Math.round(Math.random() * 100) + Math.round(Math.random() * 100) + '.',
  author_only = false
} = {}) => new Promise((resolve, reject) => {
  jsonPut('/comment', {
    parent_id,
    writ_id,
    raw_content,
    author_only
  }).then(res => {
    if (res.ok) {res.json().then(resolve).catch(e => {
      reject(new Error("no good, the json was invalid"))
    })}
  }).catch(e => {
    reject(new Error("no good, it didn't work"))
  })
})

const getComments = async (ops = {}, repeat) => {
  if (ops.path == null) ops.path = 'post:0:0/'
  if (ops.page == null) ops.page = 1
  const body = JSON.stringify(ops)
  if (typeof repeat === 'number') {
    for (let i = repeat; i-- > 0;) {
      await fetch('/comments', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body
      })
    }
    return
  }
  return await fetch('/comments', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body
  })
}

const commentRecursively = async (levels, parent_id, writ_id = parent_id) => {
  if (levels-- == 0) return true
  return commentRecursively(levels, (await putComment({ parent_id, writ_id })).data.id, writ_id)
}

const defSetup = async (cWidth = 10, reps) => {
  const writ = await pw()
  const proms = []
  while (cWidth-- > 0) proms.push(commentRecursively(9, writ.id, writ.id))
  await Promise.all(proms)
  await getComments({ path: writ.id + '/' }, reps)
}

defSetup()
}