import domlib from '/js/domlib.min.js'

const {emitter, domfn, directive, run, isRenderable, isFunc} = domlib

const route = emitter((path, view, omitSuffix) => {
    if (omitSuffix != null) path.replace(omitSuffix, '')
    if (view == null) return location.hash = path
    if (path[0] !== '#') path = '#' + path

    if (view.tagName === 'TEMPLATE') {
        view.remove()
        view = [...view.content.childNodes]
    }

    if (isRenderable(view)) {
        views[path] = isFunc(view) ? view() : view
    }

    route.handle()
})
const views = route.views = Object.create(null)
route.hash = (hash = location.hash) => hash.replace('#', '')

directive('route-link', {
    init(el, val) {
        if (el.tagName !== 'A') throw new Error('route-link is meant for actual a[href] link elements')
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
        if (el.tagName === 'TEMPLATE') return route(val, el)
        el.routeHandler = route.on.change((view, hash) => {
            if (hash === el.attr.route) { 
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

route.handle = () => {
    if (route.wasReset && route.path == location.hash) return
    if (route.path != null && route.path == location.hash) return
    let path = location.hash
    if (path.includes('/')) path = path.split('/')[0]
    if (path.includes(':')) path = path.split(':')[0]
    const view = route.views[path]
    const hash = route.hash()
    if (view == null) {
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

window.onhashchange = route.handle
run(() => {
    route.handle()
    window.dispatchEvent(new window.CustomEvent('routerReady'))
})

export default route