import d from '/js/domlib.min.js'

const app = d.emitter({d})
const df = d.domfn
const {div, article, textarea, input, a, p, button, hr, h1, h4, section, span, header} = df

const reqWithBody = (contentType = 'application/json', bodyMorpher = JSON.stringify) => method => (url, body, ops = {}) => fetch(url, {
    method,
    headers: {'Content-Type': contentType},
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

app.setupToggleSituation = (launcher, view, renderTo = 'body', {viewOutAnimation, delayRemoveMS, background} = {}) => {
    const ts = {}
    ts.clickOutHandler = d.on.pointerdown(document.body, e => {
        if (
            e.target != view &&
            !view.contains(e.target) &&
            df.hasClass(view, 'open') &&
            e.target != launcher
        ) {
            if (e.path.some(el => el instanceof Element && el.classList.contains('toast'))) return
            e.preventDefault()
            ts.toggleView(false)
            ts.clickOutHandler.off()
        }
    }).off()

    ts.launchEventHandler = d.on.pointerdown(launcher, e => {
        e.preventDefault()
        ts.toggleView()
    })

    if (background === true) {
        background = div.background_cover()
    }

    ts.toggleView = (state = !df.hasClass(view, 'open')) => {
        df.class(view, 'open', state)
        df.class(launcher, 'active', state)
        if (state) {
            if (background) d.render(background)
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
                    if (background) df.remove(background)
                    view.style.animation = ''
                    ts.launchEventHandler.on()
                })
            } else {
                if (background) df.remove(background)
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
app.components.toggleBox = (name, {id, checked, ...ops} = {}) => div.togglebox(
    tb => tb.input = input(d.merge({
        attr: {
            name,
            id,
            checked,
            type: 'checkbox'
        }
    }, ops)),
    span()
)

d.createElementPlugins.contingentVisibility = (event, el) => {
    if (d.isArr(event)) var [event, fn] = event
    if (!d.isStr(event)) return
    app.on['cv:' + event](state => {
        if (!state) {
            el.setAttribute('hidden', true)
        } else {
            el.removeAttribute('hidden')
        }
        if (d.isFunc(fn)) fn(state)
    })
}

app.dismissIcon = () => d.html( /*html*/
`<svg viewBox="0 0 20 20" xmlns="http://www.w3.org/2000/svg" style="width: 16px; height: 22px;">
<polygon fill="rgb(255, 66, 66)" points="11.649 9.882 18.262 3.267 16.495 1.5 9.881 8.114 3.267 1.5 1.5 3.267 8.114 9.883 1.5 16.497 3.267 18.264 9.881 11.65 16.495 18.264 18.262 16.497">
</polygon>
</svg>`
)

app.cv = (event, state) => {
    if (app.cv.map[event] == state) return false
    else if (state === undefined) state = !app.cv.map[event]
    app.emit['cv:' + event](app.cv.map[event] = !!state)
}
app.cv.check = event => !!app.cv.map[event]
app.cv.map = Object.create(null)

app.toastContainer = section.toast_container({
    $: 'body',
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
    const toast = div.toast[kind]({$: app.toastContainer},
        span(msg),
        span.dismiss_btn({onceclick() { dismiss() }}, app.dismissIcon())
    )
    app.toastList.add(toast)
    df.remove(toast, displayTime).then(dismiss)
    return toast
}, {
    get: (toast, kind) => toast.bind(null, kind)
})

d.run(async () => {
    try {
        await app.loadScriptsThenRunSequentially(true,
            'https://cdnjs.cloudflare.com/ajax/libs/dayjs/1.10.2/dayjs.min.js',
            'https://cdnjs.cloudflare.com/ajax/libs/dayjs/1.10.2/plugin/utc.min.js',
            'https://cdnjs.cloudflare.com/ajax/libs/dayjs/1.10.2/plugin/relativeTime.min.js'
        )
        window.dayjs.extend(window.dayjs_plugin_utc)
        window.dayjs.extend(window.dayjs_plugin_relativeTime)
        dayjs().utcOffset(2)
        app.emit('dayjsLoaded', app.dayjsLoaded = true)
    } catch (e) {
        console.error('failed to load dayjs')
    }
})

app.uponDayjsLoaded = fn => {
    if (fn) {
        return app.dayjsLoaded ? fn() : app.once.dayjsLoaded(fn)
    } else {
        return app.dayjsLoaded ? Promise.resolve(app.dayjsLoaded) : new Promise(r => app.once.dayjsLoaded(r))
    }
}

app.dateFormat = 'HH:mm a DD MMM YYYY'

app.dayjsUXTSformat = ts => {
    const date = dayjs.unix(ts).utcOffset(2)
    return date.format(app.dateFormat) + ' | ' + date.fromNow()
}

app.renderUXTimestamp = (ts, formater = app.dayjsUXTSformat, txt) => {
    if (txt == null) txt = d.txt()
    try {
        txt.textContent = formater(ts)
        if (txt.textContent.includes('minute') || txt.textContent.includes('hour')) {
            txt.updateInterval = setInterval(function update() {
                txt.textContent = formater(ts)
                if (!document.contains(txt)) clearInterval(txt.updateInterval)
                else if (txt.textContent.includes('hour')) {
                    clearInterval(txt.updateInterval)
                    txt.updateInterval = setInterval(update, 60 * 60000)
                }
            }, 60000)
        }
    } catch (e) {
        txt.textContent = new Date(ts * 1000).toLocaleString()
        app.once.dayjsLoaded(() => app.renderUXTimestamp(ts, formater, txt))
    }
    return txt
}

app.vote = async (voteType, id, up) => {
    try {
        const res = await fetch(`/${voteType}/${id}/${up == null ? 'unvote' : up ? 'upvote' : 'downvote'}`)
        return await res.json()
    } catch (e) {
        console.error('app.voteWrit error: ', e)
    }
    return false
}

app.formatVoteCount = (el, count, digits = 2) => {
    el.innerHTML = ''
    let formated = app.abreviateNum(count, digits)
    if (typeof formated === 'string' && formated.includes('.')) {
        const marker = formated[formated.length - 1]
        formated = formated.substring(0, formated.length - 1)
        const [bignum, endbits] = formated.split('.')
        formated = [
            span(bignum),
            '.',
            span.endbits(endbits),
            span.marker(marker)
        ]
    }
    d.render(formated, el)
}

app.votesUI = (voteType, {
    id,
    vote = 0,
    you_voted
}) => parentEl => {
    const votesEl = div.votes({
        async onclick(e, el) {
            if (app.user == null) {
                e.preventDefault()
                if (app.oneTimeAuthLauncher) {
                    app.oneTimeAuthLauncher.off()
                }
                try {
                    if (app.authViewToggle) {
                        app.authViewToggle.toggleView()
                    } else {
                        await import('/js/auth.min.js')
                    }
                } catch (e) {
                    if (app.oneTimeAuthLauncher) {
                        app.oneTimeAuthLauncher.on()
                    }
                }
                return
            }
            const isUp = e.target.classList.contains('up')
            const isDown = e.target.classList.contains('down')
            if (!isDown && !isUp) return
            e.target.classList.add('await-vote')
            clearInterval(el.awaitAnimTimeout)
            el.awaitAnimTimeout = setTimeout(() => {
                e.target.classList.remove('await-vote')
            }, 2500)
            const isSelected = e.target.classList.contains('selected')
            // unvote
            if (you_voted != null && isSelected) {
                const res = await app.vote(voteType, id)
                clearInterval(el.awaitAnimTimeout)
                e.target.classList.remove('await-vote')
                if (res != false) {
                    el.downvote.classList.remove('selected')
                    el.upvote.classList.remove('selected')
                    app.formatVoteCount(el.voteCount, vote = res.data)
                    you_voted = null
                }
            } else if (isUp) {
                const res = await app.vote(voteType, id, true)
                clearInterval(el.awaitAnimTimeout)
                e.target.classList.remove('await-vote')
                if (res != false) {
                    el.downvote.classList.remove('selected')
                    el.upvote.classList.add('selected')
                    app.formatVoteCount(el.voteCount, vote = res.data)
                    you_voted = true
                }
            } else if (isDown) {
                const res = await app.vote(voteType, id, false)
                clearInterval(el.awaitAnimTimeout)
                e.target.classList.remove('await-vote')
                if (res != false) {
                    el.upvote.classList.remove('selected')
                    el.downvote.classList.add('selected')
                    app.formatVoteCount(el.voteCount, vote = res.data)
                    you_voted = false
                }
            }
        }
    })

    votesEl.append(
        votesEl.upvote = span({
            class: {
                up: true,
                vote: true,
                selected: you_voted === true,
                'icon-up-open': true,
            }
        }),

        votesEl.voteCount = span.vote_count(vote),

        votesEl.downvote = span({
            class: {
                down: true,
                vote: true,
                selected: you_voted === false,
                'icon-down-open': true,
            }
        })
    )

    return votesEl
}

app.filterTagInput = (e, el) => {
    if (e.data == null) {
        el.commas = (el.value.match(/,/g) || []).length
        e.lastInput = el.value[el.value.length - 1]
        return
    }

    const vl = el.value.length
    const lastChar = vl ? el.value[vl - 1] : ''
    const penultimateChar = vl ? el.value[vl - 2] : ''

    if (e.data == ' ') {
        if (penultimateChar == ' ') {
            el.value = el.value.slice(0, -1)
        } else if (!vl || (penultimateChar != ',' && penultimateChar != '-')) {
            if (vl && penultimateChar != ' ') {
                el.value = el.value
                    .replaceAll(/, /g, ',')
                    .replaceAll(/  /g, '-')
                    .replaceAll(' ', '-')
                    .replaceAll(',', ', ')
                return
            }
            el.value = el.value.slice(0, -1) + '-'
        } else if (vl) {
            if (penultimateChar == '-') {
                if (lastChar == ' ' || lastChar == ',') {
                    el.value = el.value.slice(0, -1)
                } else{
                    el.value = el.value.slice(0, -1)
                        .replaceAll(/, /g, ',')
                        .replaceAll(/  /g, '-')
                        .replaceAll(' ', '-')
                        .replaceAll(',', ', ')

                }
            } else {
                if (penultimateChar == ',' || lastChar == ' ') {
                    el.value = el.value.slice(0, -1)
                } else {
                    el.value = el.value.replaceAll(/  /g, '-')
                }
            }
        }
    } else if (e.data == ',') {
        if (penultimateChar == ',' || penultimateChar == '-') {
            el.value = el.value.slice(0, -1)
        } else if (vl == 1) {
            el.value = ''
        } else {
            el.commas ? el.commas++ : el.commas = 1

            if (el.commas > 1) {
                el.value = [...new Set(
                    el.value.split(',')
                    .map(tag => tag.trim())
                    .filter(tag =>
                        tag.length > 1 && tag.length < 23 &&
                        tag.search(app.tagRegex) !== -1
                    )
                )].join(', ') + ', '
                el.commas = (el.value.match(/,/g) || []).length
                e.lastInput = el.value[el.value.length - 1]
            } else {
                e.lastInput = e.data
                el.value += ' '
            }
        }
    } else if (e.data == '-') {
        if (vl == 1) {
            el.value = ''
        } else {
            if (
                penultimateChar == ',' ||
                penultimateChar == '-' ||
                penultimateChar == ' ' ||
                lastChar == ' ' ||
                lastChar == ',' ||
                lastChar == '-'
            ) {
                el.value = el.value.slice(0, -1)
                return
            }
        }
    } else if (!e.data.match(/[a-zA-Z0-9]/)) {
        el.value = app.cleanseTagValue(el.value)
    }
}

app.cleanseTagValue = value => {
    value = [...new Set(value.split(',').map(tag => {
        tag = tag.trim().split('').filter(c => !!c.match(/[a-zA-Z0-9]/) || !!c.match('-')).join('')
        if (tag.length && (tag != '' || tag != ' ' || tag != '\n')) {
            while (value.includes('  ')) value = value.replaceAll(/  /g, '-')
            while (value.includes('--')) value = value.replaceAll(/--/g, '-')
            while (tag[0] == '-' || tag[0] == ' ' || tag[0] == ',') tag = tag.slice(1)
            while (tag[tag.length - 1] == '-' || tag[tag.length - 1] == ' ' || tag[tag.length - 1] == ',') tag = tag.slice(0, -1)
        }
        return tag
    }).filter(tag => tag.length < 23 && tag != '' && tag != ' ' && tag != '\n'))].join(', ')

    value = value.split('').filter((c, i) =>
            !!c.match(/[a-zA-Z0-9]/) ||
            !!(c.match('-') && (i != 0 || i != len - 1)) ||
            !!(c.match(',') && (i != 0 || i != len - 1))
        ).join('')
        .replaceAll('\n', '')
        .replaceAll(/, /g, ',')
        .replaceAll(/  /g, '-')
        .replaceAll(' ', '-')
        .replaceAll(/--/g, '-')
        .replaceAll(/---/g, '-')
        .replaceAll(',', ', ')

    value = value.replace(/-$/g, '')

    if (value.includes('-,')) value = value.replaceAll(/,-/g, ',')
    while (value.includes('--')) value = value.replaceAll(/--/g, '-')
    while (value.includes(',,')) value = value.replaceAll(/,,/g, ',')
    while (value.includes(', , ')) value = value.replaceAll(/, , /g, ', ')
    while (value.includes('  ')) value = value.replaceAll(/,,/g, ',')

    return value.length ? value : ''
}

app.tagRegex = /^[a-zA-Z0-9-]+$/

app.tabView = ({
    $,
    tabs = [],
    attacher = 'after',
    ...ops
}) => {
    const list = header({
        onclick(e) {
            if (e.target.classList.contains('tab-name') && !e.target.classList.contains('active')) {
                if (activeTab) {
                    activeTab.name.classList.remove('active')
                    activeTab = views[e.target.textContent]
                    activeTab.name.classList.add('active')
                    viewer.innerHTML = ''
                    d.render(activeTab.view, viewer)
                }
            }
        }
    })
    const views = Object.create(null)
    tabs.map(({
        name,
        view
    }) => {
        views[name] = {
            name: div.tab_name(name),
            view
        }
        list.append(views[name].name)
    })

    let activeTab = views[tabs[0].name]
    activeTab.name.classList.add('active')

    const viewer = div(activeTab.view)

    const element = article.tab_view(ops, list, viewer)
    if ($) d.render(element, $, attacher)

    return {
        element,
        get tabs() {
            return tabs
        },

        get views() {
            return views
        },

        add(name, view) {
            tabs.push({name, view})
            views[name] = {
                name: div.tab_name({$: list}, name),
                view
            }
        },

        remove(name) {
            tabs = tabs.filter(tab => tab.name != name)
            df.remove(views[name].name)
            delete views[name]
        },

        set active(name) {
            if (views[name]) {
                activeTab.name.classList.remove('active')
                activeTab = views[name]
                activeTab.name.classList.add('active')
                viewer.innerHTML = ''
                d.render(activeTab.view, viewer)
            }
        },

        get active() {
            return activeTab
        }
    }
}

const isURL = url => {
    try {
        new URL(url);
        return true;
    } catch (e) {}
    return false;
}

window.app = app
export default app
