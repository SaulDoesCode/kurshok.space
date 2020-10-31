import domlib from '/js/domlib.min.js'

const app = domlib.emitter()
const d = app.d = domlib, df = domlib.domfn
const reqWithBody = (contentType = 'application/json', bodyMorpher = JSON.stringify) => method => (url, body, ops = {}) => fetch(url, {
    method,
    headers: {
        'Content-Type': contentType
    },
    body: bodyMorpher(body),
    ...ops,
})
const jsonHTTPMethod = reqWithBody()
const textHTTPMethod = reqWithBody('text/plain', b => b)
app.jsonPost = jsonHTTPMethod('POST')
app.jsonPut = jsonHTTPMethod('PUT')
app.jsonDelete = jsonHTTPMethod('DELETE')
app.txtPost = textHTTPMethod('POST')
app.txtPut = textHTTPMethod('PUT')
app.txtDelete = textHTTPMethod('DELETE')

const wq = endpoint => async (query = {}) => {
    if (isNaN(query.page)) query.page = 0
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


app.loadScriptsThenRunSequentially = (saveInLS, ...scripts) => new Promise(async (resolve, reject) => {
    const fizzout = setTimeout(reject, 3000 * scripts.length)
    const lastScript = scripts[scripts.length - 1]
    let scriptage = ''
    if (saveInLS && lastScript in localStorage) {
        scriptage = localStorage.getItem(lastScript)
    } else {
        const cache = {}, fetches = []
        for (const script of scripts) fetches.push(
            fetch(script)
                .then(res => res.text())
                .then(txt => {cache[script] = txt})
        )
        await Promise.all(fetches)
        for (const script in cache) {
            scriptage += '\n;' + cache[script]
        }
        scriptage += `\n; app.emit("doneLoading:${lastScript}");`
        if (saveInLS) localStorage.setItem(lastScript, scriptage)
    }

    app.once['doneLoading:' + lastScript](resolve)
    df.script({$: document.head}, scriptage)
    clearTimeout(fizzout)
})

app.loadStyle = async (url, cache) => {
    let txt
    if (cache && url in localStorage) {
        txt = localStorage.getItem(url)
    } else {
        txt = await (await fetch(url)).text()
        if (cache) {
            localStorage.setItem(url, txt)
        }
    }

    df.style({$: document.head}, txt)
}

const abreviateNum_units = ['k', 'M', 'G', 'T', 'P', 'E', 'Z', 'Y']
app.abreviateNum = (num, digits) => {
    for (let decimal, i = abreviateNum_units.length - 1; i >= 0; i--) {
        decimal = Math.pow(1000, i + 1)
        if (num <= -decimal || num >= decimal)
            return +(num / decimal).toFixed(digits) + abreviateNum_units[i]
    }
    return num
}

app.components = {}
app.components.toggleBox = (name, {id, checked, ...ops} = {}) => df.div({
    class: 'togglebox',
},
    tb => tb.input = df.input(d.merge({
        attr: {
            name,
            id,
            checked,
            type: 'checkbox'
        }
    }, ops)),
    df.span()
)

domlib.createElementPlugins.contingentVisibility = (event, el) => {
    if (domlib.isArr(event)) var [event, fn] = event
    if (!domlib.isStr(event)) return
    app.on['cv:' + event](state => {
        if (!state) {
            el.setAttribute('hidden', true)
        } else {
            el.removeAttribute('hidden')
        }
        if (domlib.isFunc(fn)) fn(state)
    })
}

app.cv = (event, state) => {
    if (app.cv.map[event] == state) return false
    else if (state === undefined) state = !app.cv.map[event]
    app.emit['cv:' + event](app.cv.map[event] = !!state)
}
app.cv.check = event => !!app.cv.map[event]
app.cv.map = Object.create(null)

app.toastContainer = df.section({
    $: 'body',
    class: 'toast-container',
    contingentVisibility: 'gotToast'
})

app.cv('gotToast', false)

app.toastList = new Set()
app.toast = new Proxy((kind, msg, displayTime = 15000) => {
    app.cv('gotToast', true)
    const dismiss = () => {
        df.remove(toast)
        app.toastList.delete(toast)
        if (app.toastList.size == 0) app.cv('gotToast', false)
    }
    const toast = df.div({
        $: app.toastContainer,
        class: 'toast ' + kind
    },
        df.span(msg),
        df.span({
            class: 'dismiss-btn',
            onceclick() {
                console.log('moo')
                dismiss()
            }
        },
            domlib.html(/*html*/
`<svg viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" style="width: 16px; height: 22px;">
    <polygon fill="rgb(255, 66, 66)" points="11.649 9.882 18.262 3.267 16.495 1.5 9.881 8.114 3.267 1.5 1.5 3.267 8.114 9.883 1.5 16.497 3.267 18.264 9.881 11.65 16.495 18.264 18.262 16.497">
    </polygon>
</svg>`
            )
        )
    )
    app.toastList.add(toast)
    df.remove(toast, displayTime).then(dismiss)
    return toast
}, {
    get: (toast, kind) => toast.bind(null, kind)
})

window.app = app
export default app
