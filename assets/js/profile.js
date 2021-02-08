import app from '/js/site.min.js'
const d = app.d, df = d.domfn
const {header, section, span, p, textarea, input, button, b, div, article} = df
const {txt} = d

const profileLauncher = d.query('.profile-launcher')

const profileView = app.profileView = section({
    class: 'profile-view'
}, pv => [
    pv.profileHeader = header(
        span.username(b(pv.usernameView = txt(app.user.username)), "'s profile"),
        span.divider('|'),
        span.handle('handle: @', pv.handleView = txt(app.user.handle))
    ),
    pv.logoutBtn = button.logout_btn('Log out')
])

const {logoutBtn, profileHeader, usernameView, handleView} = profileView

app.profileViewToggle = app.setupToggleSituation(
    profileLauncher,
    profileView,
    'body',
    {background: true}
)

d.once.click(logoutBtn, async e => {
    const res = await (await fetch('/auth', {method: 'DELETE'})).json()
    df.remove(logoutBtn)
    profileView.append(div(res.status))
    setTimeout(() => window.location.reload(), 1000)
})

let udonce = false
app.userDescription = async fn => {
    if (fn == null) fn = desc => Promise.resolve(desc)
    if (app.user.description != null) return fn(app.user.description)
    if (!udonce) {
        udonce = true
        const res = await (await fetch('/user/' + app.user.id + '/description')).json()
        if (res.ok) {
            app.emit.userDescription(app.user.description = res.data)
            return fn(app.user.description)
        }
    } else {
        app.once.userDescription(fn)
        return Promise.resolve(app.user.description)
    }
    return Promise.reject(503)
}
app.userDescription()

const descriptionTabs = app.tabView({
    $: profileHeader,
    attacher: 'after',
    tabs: [
        {
            name: 'description',
            view: div.description(d => {
                app.userDescription(desc => {
                    p({$: d}, desc)
                })
            })
        },
        {
            name: 'change',
            view: div.change_description(d => [
                d.editor = textarea({
                    attr: {
                        max: 300,
                        min: 3
                    }
                }, ta => {
                    app.userDescription(desc => {
                        ta.value = desc
                    })
                }),
                button({
                    async onclick() {
                        let desc = d.editor.value.trim()
                        if (desc.length > 299 || desc.length < 4) {
                            app.toast.error('Description is too short')
                            return
                        }
                        if (app.user.description === desc) {
                            app.toast.error('User description was not altered')
                            return
                        }
                        const res = await (await app.jsonPost('/user/change/description', desc)).json()
                        if (res.ok) {
                            app.user.description = desc
                            descriptionTabs.active = 'description'
                            descriptionTabs.views.description.view.innerHTML = ''
                            descriptionTabs.views.description.view.append(
                                p(app.user.description)
                            )
                            app.toast.normal(res.status)
                        } else {
                            app.toast.error(res.status)
                        }
                    }
                }, 'change')
            ])
        }
    ]
})

const handleTabs = app.tabView({
    $: profileHeader,
    attacher: 'after',
    tabs: [
        {
            name: 'handle',
            view: div({class: 'handle'}, span(app.user.handle))
        },
        {
            name: 'change',
            view: div({
                class: 'change-handle',
            }, d => [
                d.editor = input({
                    value: app.user.handle,
                    attr: {
                        type: 'text',
                        max: 300,
                        min: 3
                    }
                }),
                button({
                    async onclick() {
                        let handle = d.editor.value.trim()
                        if (handle.length > 30 || handle.length < 3) {
                            app.toast.error('User handle is either too short or too long')
                            return
                        }
                        if (app.user.handle === handle) {
                            app.toast.error('User handle was not altered')
                            return
                        }
                        const res = await (await app.jsonPost('/user/change/handle', handle)).json()
                        if (res.ok) {
                            app.user.handle = handle
                            handleTabs.active = 'handle'
                            handleTabs.views.handle.view.innerHTML = ''
                            handleTabs.views.handle.view.append(p(app.user.handle))
                            handleView.textContent = app.user.handle
                            app.toast.normal(res.status)
                        } else {
                            app.toast.error(res.status)
                        }
                    }
                }, 'change')
            ])
        }
    ]
})

const usernameTabs = app.tabView({
    $: profileHeader,
    attacher: 'after',
    tabs: [{
        name: 'username',
        view: div.username(span(app.user.username))
    }, {
        name: 'change',
        view: div.change_username(d => [
            d.editor = input({
                value: app.user.username,
                attr: {
                    type: 'text',
                    max: 50,
                    min: 3
                }
            }),
            button({
                async onclick() {
                    let username = d.editor.value.trim()
                    if (username.length > 30 || username.length < 3) {
                        app.toast.error('User handle is too short')
                        return
                    }
                    if (app.user.username === username) {
                        app.toast.error('Username was not altered')
                        return
                    }
                    const res = await (await app.jsonPost('/user/change/username', username)).json()
                    if (res.ok) {
                        app.user.username = username
                        usernameTabs.active = 'username'
                        usernameTabs.views.username.view.innerHTML = ''
                        usernameTabs.views.username.view.append(p(app.user.username))
                        usernameView.textContent = app.user.username
                        app.toast.normal(res.status)
                    } else {
                        app.toast.error(res.status)
                    }
                }
            }, 'change')
        ])
    }]
})