import domlib from '/js/domlib.min.js'

const app = domlib.emitter()
const d = app.d = domlib, df = domlib.domfn
const jsonHTTPMethod = method => (url, body) => fetch(url, {
    method,
    headers: {'Content-Type': 'application/json'},
    body: JSON.stringify(body)
})
app.jsonPost = jsonHTTPMethod('POST')
app.jsonPut = jsonHTTPMethod('PUT')

app.writQuery = async (query = {}) => {
    if (isNaN(query.page)) query.page = 1
    if (!query.kind) query.kind = 'post'
    const res = await app.jsonPost('/writs', query)
    return await res.json()
}
app.editableWritQuery = async (query = {}) => {
    if (isNaN(query.page)) query.page = 1
    if (!query.kind) query.kind = 'post'
    const res = await app.jsonPost('/editable-writs', query)
    return await res.json()
}

app.setupToggleSituation = (launcher, view, renderTo = 'body') => {
    const ts = {}

    ts.clickOutHandler = d.on.pointerdown(document.body, e => {
        if (
            e.target != view &&
            !view.contains(e.target) &&
            df.hasClass(view, 'open') &&
            e.target != launcher
        ) {
            e.preventDefault()
            ts.toggleView(false)
            ts.clickOutHandler.off()
        }
    }).off()

    ts.launchEventHandler = d.on.pointerdown(launcher, e => {
        e.preventDefault()
        ts.toggleView()
    })

    ts.toggleView = (state = !df.hasClass(view, 'open')) => {
        df.class(view, 'open', state)
        df.class(launcher, 'active', state)
        if (state) {
            d.render(view, renderTo)
            ts.clickOutHandler.on()
        } else {
            df.remove(view)
        }
    }

    ts.on = true
    ts.toggle = (state = !ts.on) => {
        ts.toggleView(state)
        if (!state) {
            ts.launchEventHandler.off()
            ts.clickOutHandler.off()
            df.remove(view)
        }
        return ts.on = state
    }

    return ts
}

window.app = app
export default app
