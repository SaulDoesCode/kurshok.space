import domlib from '/js/domlib.min.js'

const {emitter, domfn, directive, run, isRenderable, isFunc} = domlib

const route = emitter(async (path, view, omitSuffix, noHandle) => {
    if (omitSuffix != null) path.replace(omitSuffix, '')
    if (path[0] !== '#') path = '#' + path
    if (view == null) return location.hash = path

    if (views[path]) throw new Error('route already exists')

    if (view.tagName === 'TEMPLATE') {
        view.remove()
        view = [...view.content.childNodes]
    }

    if (isFunc(view)) {
        view = view.constructor.name === 'AsyncFunction' ? (await view()) : view()
    }
    if (view instanceof Promise) {
        view = await view
    }
    if (isRenderable(view) || isFunc(view)) {
        views[path] = view
    }
    if (!noHandle) route.handle()
})
const views = route.views = Object.create(null)
route.hash = (hash = location.hash) => hash.replace('#', '')

directive('route-link', {
    init(el, val) {
        if (el.tagName !== 'A') throw new Error('route-link is meant for actual a[href] / link elements')
        el.routeLink = route.on.change(() => el.classList.toggle(
            'active-route',
            location.hash === (el.href = '#' + el.getAttribute('route-link'))
        ))
        el.classList.toggle('active-route', location.hash === (el.href = '#' + val))
    },
    remove: (el, val) => el.routeLink.off()
})

directive('route', {
    init(el, val) {
        if (el.tagName === 'TEMPLATE') {
            return route(val, el)
        }
        if (el.tagName === 'TEMPLATE-FILE') {
            try {
                if (val[0] !== '#') val = '#' + val
                const src = el.getAttribute('src')
                const immediate = el.getAttribute('immediate')
                if (immediate != null) {
                    const prerender = immediate.length > 1
                    fetch(src).then(res => res.text()).then(txt => {
                        if (prerender) {
                            route(val, domlib.html(txt), null, true)
                        } else {
                            route(val, () => () => domlib.html(txt), null, true)
                        }
                        route.handle()
                    })
                } else {
                    let once
                    let view = async () => {
                        if (!once) {
                            once = true
                            return view = domlib.html(await (await fetch(src)).text())
                        }
                        return view
                    }
                    route(val, () => view)
                }
                domlib.domfn.remove(el)
            } catch(e) {
                console.error(`invalid <template-file route="${val}" src="???">`, e)
            }
            return
        }

        el.routeHandler = route.on.change((view, hash) => {
            if (hash === el.getAttribute('route')) { 
                el.innerHTML = ''
                domfn.append(el, domfn.html(view))
            } else {
                el.textContent = ''
            }
        })
    },
    remove(el, val) {
        if (el.routeHandler) {
            el.routeHandler.off()
            el.textContent = ''
        }
    }
})

directive('route-active', {
    init(el, val) {
        el.routeHandler = route.on.change((view, hash) => {
            el.setAttribute('route-active', hash)
            el.innerHTML = ''
            domfn.append(el, domlib.html(view))
        })
        el.routeOffHandler = route.on.off(hash => {
            el.innerHTML = route.view404(hash)
        })
    },
    remove(el, val) {
        el.routeHandler.off()
        el.textContent = ''
    }
})

route.view404 = hash => `<br><header>404 - ${hash} | No view for this route</header>`

domlib.route = route

const handled = Object.create(null)

route.handle = async () => {
    if (route.wasReset && route.path == location.hash) return
    if (route.path != null && route.path == location.hash) return
    let path = location.hash
    if (path.includes('/')) path = path.split('/')[0]
    if (path.includes(':')) path = path.split(':')[0]
    let view = route.views[path]
    const hash = route.hash()

    if (isFunc(view) && !handled[path]) {
        handled[path] = true
        view = route.views[path] = view.constructor.name === 'AsyncFunction' ? (await view()) : view()
    }
    if (view instanceof Promise) {
        view = await view
    }

    if (view == null || isFunc(view)) {
        if (route.path != null) {
            location.hash = route.path
            route.wasReset = true
            return
        }
        route.emit.off(hash)
        return
    }
    route.lastPath = route.path
    route.path = location.hash
    route.active = view
    route.emit.change(view, hash, route)
    route.emit[hash](view, route)
    route.emit[path.slice(1)](hash, view, route)
}

route.whenActive = (hash, fn, once) => run(() => {
    if (hash[0] !== '#') hash = '#' + hash
    let view = route.views[hash]
    if (location.hash === hash && view != null) {
        fn(view, route, hash)
        if (once) return
    }
    hash = route.hash(hash)
    route[once ? 'once' : 'on'][hash](view => {
        fn(view, route, hash)
    })
})

window.addEventListener('hashchange', route.handle)
run(() => {
    route.handle()
    window.dispatchEvent(new window.CustomEvent('routerReady'))
})

export default route