import app from '/js/site.min.js'
import route from '/js/router.min.js'
const d = app.d, df = d.domfn
const {div, input, button, article, a, p, hr, h1, h4, section, span, header} = df

const mainView = d.query('main[route-active]')

const postListView = div({class: 'post-list'})
const contentDisplay = section({class: 'posts'},
    postListView
)
app.postDisplay = section({class: 'full post'},
pd => {
    pd.parts = d.h /* html */ `
        <header class="post-header">
            <div>
                <h3 class="post-title" ref="title"></h3>
                <div class="author-name" ref="author"></div>
                <div> ● </div>
                <time class="posted" ref="date"></time>
                <div class="tags" ref="tags"></div>
            </div>
        </header>
        <article class="content" ref="content"></article>
    `.renderCollect(pd)
})

app.postQuery = {
    page: 0,
    amount: 8,
    kind: 'post',
    public: true
}

const postFilterView = div({class: 'post-filter'}, pf => [
    section({class: 'tag-filter'}, 
        header('Tag filter'),
        pf.tagInput = input({
            attr: {
                type: 'text',
                name: 'tags',
                pattern: '[a-zA-Z0-9-, ]+'
            },
            onkeydown(e, el) {
                if (e.key == 'Enter' && el.value.length > 1) {
                    let value = el.value.replace(' ', '')
                    if (value.includes(',')) {
                        value = value.split(',')
                    }
                    postFilterView.collectTags(value)
                    el.value = ''
                    el.commas = 0
                    el.lastInput = undefined
                }
            },
            oninput(e, el) {
                if (e.data == null) {
                    el.commas = (el.value.match(/,/g) || []).length
                    e.lastInput = el.value[el.value.length - 1]
                    return
                }

                if (e.data == ' ') {
                    const vl = el.value.length
                    console.log(`el.value = ${el.value} && len = ${vl}`)
                    if (!vl || el.value[vl - 2] != ',') {
                        el.value = el.value.slice(0, -1)
                    }
                } else if (e.data == ',') {
                    if (el.lastInput == ',' || el.lastInput == '-') {
                        el.value = el.value.slice(0, -1)
                    } else if (el.value.length === 1) {
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
                            console.log('it happened')
                        } else {
                            e.lastInput = e.data
                        }
                    }
                } else if (e.data == '-') {
                    if (el.value.length == 1) {
                        el.value = ''
                    } else if (el.value[el.value.length - 2] == ',') {
                        el.value = el.value.slice(0, -1)
                    } else {
                        el.lastInput = '-'
                    }
                } else if (!e.data.match(/[a-zA-Z0-9, -]/)) {
                    el.value = el.value.slice(0, -1)
                } else {
                    el.lastInput = e.data
                }
            }
        }),
        pf.tagListContainer = div({
            attr: {hidden: true},
            class: 'tag-list-container'
        },
            pf.tagListDisplay = div({
                class: 'list',
            }),
            button({
                onclick() {
                    pf.runQuery()
                }
            }, 'query'),
            pf.clearBtn = button({
                attr: {hidden: true},
                onclick(e, el) {
                    pf.tags = []
                    delete app.postQuery.tags
                    app.postQuery.page = 0
                    pf.collectTags()
                    app.postFilter({})
                    el.setAttribute('hidden', true)
                }
            }, 'clear')
        )
    )
])

postFilterView.runQuery = (with_tag) => {
    postFilterView.collectTags(with_tag)
    if (postFilterView.tags && postFilterView.tags.length) {
        app.filterQuery = true
        app.postFilter({
            tags: postFilterView.tags
        })
        postFilterView.clearBtn.removeAttribute('hidden')
    }
}

postFilterView.collectTags = (newTag) => {
    if (!postFilterView.tags) postFilterView.tags = []
    if (d.isArr(newTag)) {
        newTag.forEach(t => {
            if (typeof t == 'string' && t.length > 1) {
                postFilterView.tags.push(t)
            }
        })
    } else if (typeof newTag == 'string' && newTag.length > 1) {
        postFilterView.tags.push(newTag)
    }

    postFilterView.tags = [...new Set(postFilterView.tags)]
        .map(tag => tag.trim())
        .filter(tag => 
            tag.length > 1 && tag.length < 23 &&
            tag.search(app.tagRegex) !== -1
        )
    
    df.attrToggle(postFilterView.tagListContainer, 'hidden', !postFilterView.tags.length)

    const alreadyThere = []
    d.each(postFilterView.tagListDisplay.children, el => {
        if (!postFilterView.tags.includes(el.title)) {
            df.remove(el)
        } else {
            alreadyThere.push(el.title)
        }
    })

    postFilterView.tags.forEach(tag => {
        if (alreadyThere.includes(tag)) return

        span({
            $: postFilterView.tagListDisplay,
            class: 'tag',
            title: tag,
            onclick(e, el) {
                postFilterView.tags = postFilterView.tags.filter(t => t != tag)
                df.remove(el)

                df.attrToggle(postFilterView.tagListContainer, 'hidden', !postFilterView.tags.length)
            }
        }, tag)
    })

    return postFilterView.tags
}

app.tagRegex = /^[a-zA-Z0-9-]+$/

route('posts', [contentDisplay, postFilterView])

if (location.hash == '' || location.hash == '#') {
    location.hash = 'posts'
    route.handle()
}

route('post', app.postDisplay)

route('no-content', div({class: 'no-content'},
    h1('Sorry folks, no content. This website is barren.'),
    button({
        onclick() {
            location.hash = 'posts'
            window.location = '/'
        }
    }, 'Reload page, maybe it helps.')
))

const postNavView = d.html(/* html */`
    <nav class="post-nav">
        <button class="post-back-btn" onclick="location.hash = app.fancyHash || 'posts'">
            <span class="icon-left-open"></span>
            Back to Post List
        </button>
    </nav>
`)

const quickScroll = d.html( /* html */ `
<nav class="quick-scroll">
    <div class="to-top icon-up-open" onclick="window.scrollTo({top: 0, left: 0, behavior: 'smooth'})">
    </div>
    <div class="to-comments icon-comment" onclick="document.querySelector('#comments').scrollIntoView({behavior: 'smooth'})">
    </div>
</nav>`)

route.on.post(async hash => {
    setTimeout(() => {
        if (!app.posts[hash] && app.failedToFetchPosts) {
            location.hash = 'no-content'
            route.handle()
        }
    }, 1200)
    await app.afterPostsInitialization()
    const post = app.activePost = app.posts[hash]
    if (post == null) {
        location.hash = 'no-content'
        route.handle()
        return
    }
    const {title, tags, author, date, content} = app.postDisplay.parts
    title.textContent = post.title
    date.innerHTML = ''
    d.render(app.renderUXTimestamp(post.posted), date)
    tags.innerHTML = ''
    post.tags.map(tag => span({
        $: tags,
        class: 'tag',
        attr: {title: tag},
    }, tag))
    author.textContent = 'By ' + post.author_name
    content.innerHTML = 'Content loading...'
    if (app.commentsDisplay) df.remove(app.commentsDisplay)
    df.prepend(mainView, postNavView)
    const postContent = await app.fetchPostContent(post.id)
    content.innerHTML = ''
    if (post == app.activePost) {
        d.render(d.html(postContent), content)
        setTimeout(() => d.queryAll('.content code', content).forEach(el => {
            el.classList.add('language-rust')
        }), 60)
        d.render(quickScroll)
    }
    app.emit.postRendered(post, hash)
})

route.on.change(() => {
    if (!location.hash.includes('post:')) {
        df.remove(quickScroll)
    }
})

const publicPost = w => div({
    $: postListView,
    class: 'post',
    attr: {pid: w.id},
    onclick(e, el) {
        if (e.target.className.includes('vote')) return
        if (e.target.classList.contains('tag')) {
            let tag = e.target.getAttribute('title')
            if (tag != null) {
                postFilterView.runQuery(tag)
            }
            return
        }
        location.hash = w.id
    }
}, pubPost => {
    const titleEl = h4(w.title)
    app.on('postEdit:' + w.id, p => {
        if (p === false) {
            df.remove(pubPost)
            app.postPages[w.page] = app.postPages[w.page].filter(id => id != w.id)
            delete app.posts[w.id]
            if (location.hash.includes(w.id)) {
                location.hash = 'posts'
            }
        } else {
            if (location.hash.includes('no-content')) {
                location.hash = 'posts'
                route.handle()
            }
            titleEl.textContent = app.posts[w.id].title = p.title
            app.posts[w.id].commentable = p.commentable
        }
    })

    return [
        header(
            div({class: 'title'},
                titleEl,
                div({class: 'author-name'}, `By ${w.author_name}`)
            ),
            div({class: 'posted'}, app.renderUXTimestamp(w.posted)),
            df.hr(),
            app.votesUI('writ', w),
            div({class: 'tags'},
                w.tags.map(t => span({
                    class: 'tag',
                    attr: {title: t}
                }, t))
            )
        )
    ]
});

app.fetchPostContent = async id => {
    if (app.posts[id] && app.posts[id].content != null) {
        return app.posts[id].content
    }
    const res = await fetch('/post-content/' + id)
    const data = await res.json()
    if (!data.ok) throw new Error(data.status)
    return app.posts[id].content = data.data
}

app.posts = Object.create(null)
app.postPages = Object.create(null)

app.postPaginationView = section({
    class: 'post-pagination',
},
    app.postPageBackBtn = div({
        class: 'page-back',
        contingentVisibility: 'pageNot0',
        onclick(e) {
            app.fetchPosts(app.activePostPage - 1)
        }
    }, 
        '<<',
    ),
    app.pageNumView = div({class: 'page-num'}),
    app.postPageForwardBtn = div({
            class: 'page-forward',
            onclick(e) {
                app.fetchPosts(app.activePostPage + 1)
            }
        },
        '>>',
    ),
)

app.fetchPosts = async (...args) => {
    if (args.length) {
        if (d.isNum(args[0])) {
            app.postQuery.page = args[0]
            if (d.isNum(args[1])) {
                app.postQuery.amount = args[1]
            }
        }
    }

    try {
        if (!app.postQuery.tags.length) {
            delete app.postQuery.tags
        }
    } catch(e) {}

    if (route.hash() == 'no-content') {
        if (!app.failedToFetchPosts) {
            location.hash = 'posts'
            route.handle()
            console.log('false alarm')
        } else {
            location.hash = 'posts'
            route.handle()
            console.warn('not sure what happened')
        }
    }

    let writs
    if (d.isArr(app.postPages[app.postQuery.page])) {
        postListView.innerHTML = ''
        let i = 0
        for (const wid of app.postPages[app.postQuery.page]) {
            publicPost(app.posts[wid])
            if (++i >= app.postQuery.amount) break
        }
    } else {
        try {
            writs = await app.writQuery({
                with_content: false,
                ...app.postQuery
            })
            if (!d.isArr(writs)) {
                if (app.postQuery.page == 0 && !app.filterQuery) {
                    mainView.innerHTML = ''
                    app.failedToFetchPosts = true
                    location.hash = 'no-content'
                    route.handle()
                }
                console.error(writs)
                throw new Error(writs.status)
            } else {
                app.failedToFetchPosts = false
            }
        } catch(e) {
            app.postPageForwardBtn.style.color = 'red'
            app.postPageForwardBtn.textContent = '404'
            setTimeout(() => {
                app.postPageForwardBtn.style.color = ''
                app.postPageForwardBtn.textContent = '>>'
            }, 3000)

            if (location.hash.includes('post:')) {
                if (app.postQuery.page == 0) {
                    app.failedToFetchPosts = true
                    location.hash = 'no-content'
                    route.handle()
                } else {
                    location.hash = 'posts'
                    route.handle()
                }
            } else if (app.postQuery.page == 0 && !app.filterQuery) {
                mainView.innerHTML = ''
                app.failedToFetchPosts = true
                location.hash = 'no-content'
                route.handle()
            }

            return
        }
        app.postPages[app.postQuery.page] = []
        postListView.innerHTML = ''

        if (writs.length >= 5) {
            d.render(app.postPaginationView, contentDisplay)
        }

        writs //.sort((w0, w1) => w1.posted - w0.posted)
            .forEach(w => {
                w.page = app.postQuery.page
                app.postPages[app.postQuery.page].push(w.id)
                publicPost(app.posts[w.id] = w)
            })
    }
    app.emit.activePostPage(app.activePostPage = app.postQuery.page)
    app.pageNumView.textContent = app.postQuery.page
    app.cv('pageNot0', app.postQuery.page != 0)
    if (!app.postsInitialized) {
        app.emit.postsInitialized(app.postsInitialized = true)
        app.loadStyle('https://cdnjs.cloudflare.com/ajax/libs/prism/1.21.0/themes/prism-tomorrow.min.css', true)
        await import('/js/comments.min.js')
    }
}

app.fetchPosts()

app.on.newPost(() => {
    app.posts = Object.create(null)
    app.postPages = Object.create(null)
    postListView.innerHTML = ''
    app.fetchPosts()
})

app.postFilter = async filter => {
    if (filter.tags) {
        app.postQuery.tags = filter.tags
    }

    app.posts = Object.create(null)
    app.postPages = Object.create(null)
    postListView.innerHTML = ''
    await app.fetchPosts()

    if (!Object.keys(app.posts).length) {
        const foundNada = df.h2('404 - No matching posts found :(')
        d.render(foundNada, 'section.posts', 'prepend')
        d.once.click(postFilterView, () => {
            df.remove(foundNada)
        })
    }

    app.filterQuery = false
}

app.afterPostsInitialization = fn => app.postsInitialized ?
    (fn != null ? fn() : Promise.resolve(true)) : 
    fn != null ?
        app.once.postsInitialized(fn) :
        new Promise(app.once.postsInitialized)