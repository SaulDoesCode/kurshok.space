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

const wq = endpoint => async (query = {}) => {
    if (isNaN(query.page)) query.page = 1
    if (!query.kind) query.kind = 'post'
    const res = await app.jsonPost(endpoint, query)
    return await res.json()
}
app.writQuery = wq('/writs')
app.editableWritQuery = wq('/editable-writs')

app.toggleSituations = {list: [], active: null}

app.setupToggleSituation = (launcher, view, renderTo = 'body', {viewOutAnimation, delayRemoveMS} = {}) => {
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
            if (app.toggleSituations.active) {
                app.toggleSituations.active.toggleView(false)
            }
            app.toggleSituations.active = ts
        } else {
            if (app.toggleSituations.active == ts) {
                app.toggleSituations.active = null
            }
            if (delayRemoveMS != null) {
                view.style.animation = viewOutAnimation
                ts.launchEventHandler.off()
                df.remove(view, delayRemoveMS).then(() => {
                    view.style.animation = ''
                    ts.launchEventHandler.on()
                })
            } else {
                df.remove(view)
            }
        }
    }

    app.toggleSituations.list.push(ts)
    return ts
}

window.app = app
export default app
