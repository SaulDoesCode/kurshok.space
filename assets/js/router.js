import domlib from '/js/domlib.min.js'

const router = domlib.emitter()

router.mode = app.routerMode || window.history.pushState ? 'history' : 'hash'

window.addEventListener('hashchange', router.check)

router.root = '/'

router.clearSlashes = path => path.toString()
    .replace(/\/$/, '')
    .replace(/^\//, '')

router.getFragment = () => {
    let fragment = ''
    if (router.mode === 'history') {
        fragment = router.clearSlashes(decodeURI(window.location.pathname + window.location.search))
        fragment = fragment.replace(/\?(.*)$/, '')
        fragment = router.root !== '/' ? fragment.replace(router.root, '') : fragment
    } else {
        const match = window.location.href.match(/#(.*)$/)
        fragment = match ? match[1] : ''
    }
    return router.clearSlashes(fragment)
}

router.navigate = (path = '') => {
    if (router.mode === 'history') {
        window.history.pushState(null, null, router.root + router.clearSlashes(path))
    } else {
        window.location.href = `${window.location.href.replace(/#(.*)$/, '')}#${path}`
    }
    router.check()
    return router
}

router.check = () => {
    if (router.current === router.getFragment()) return
    router.current = router.getFragment()
    console.log(router.current)
    router.emit(router.current)
}

const pushState = window.history.pushState
window.history.pushState = (data, title, url) => {
    pushState.call(history, data, title, url)
    router.check()
}

export default router