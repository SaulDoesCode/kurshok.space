import app from '/js/site.min.js'
import route from '/js/router.min.js'
const d = app.d, df = d.domfn
const {div, button, h1, h4, section, span, header} = df


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
                <div class="posted" ref="date"></div>
                <div class="tags" ref="tags"></div>
            </div>
        </header>
        <article class="content" ref="content"></article>
    `.renderCollect(pd)
})


route('posts', contentDisplay)
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

d.run(async () => {
    try {
        await app.loadScriptsThenRunSequentially(true,
            'https://cdnjs.cloudflare.com/ajax/libs/dayjs/1.8.36/dayjs.min.js',
            'https://cdnjs.cloudflare.com/ajax/libs/dayjs/1.8.36/plugin/utc.min.js',
            'https://cdnjs.cloudflare.com/ajax/libs/dayjs/1.8.36/plugin/relativeTime.min.js'
        )
        window.dayjs.extend(window.dayjs_plugin_utc)
        window.dayjs.extend(window.dayjs_plugin_relativeTime)
        dayjs().utcOffset(2)
        app.emit('dayjsLoaded', app.dayjsLoaded = true)
    } catch (e) {}
})

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
    post.tags.map(tag => df.span({$: tags, attr:{title: tag}, class:'tag'}, tag))
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

app.votesUI = (voteType, {id, vote = 0, you_voted}) => parentEl => {
    const votesEl = div({
            class: 'votes',
            async onclick(e, el) {
                if (app.user == null) {
                    e.preventDefault()
                    app.oneTimeAuthLauncher.off()
                    try {
                        if (app.authViewToggle) {
                            app.authViewToggle.toggleView()
                        } else {
                            await import('/js/auth.min.js')
                        }
                    } catch (e) {
                        app.oneTimeAuthLauncher.on()
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
        }
    )

    votesEl.append(
        votesEl.upvote = span({
            class: {
                up: true,
                vote: true,
                selected: you_voted === true,
                'icon-up-open': true,
            }
        }),

        votesEl.voteCount = span({
            class: 'vote-count'
        }, vote),

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

const publicPost = w => div({
    $: postListView,
    class: 'post',
    attr: {pid: w.id},
    onclick(e, el) {
        if (e.target.className.includes('vote')) return
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
        app.votesUI('writ', w),
        header(
            div({class: 'title'},
                titleEl,
                div({class: 'author-name'}, `By ${w.author_name}`)
            ),
            div(
                div({class: 'posted'}, app.renderUXTimestamp(w.posted)),
                div({class: 'tags'},
                    w.tags.map(t => span({
                        class: 'tag',
                        attr: {title: t}
                    }, t))
                )
            )
        )
    ]
});

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
            span({class: 'endbits'}, endbits),
            span({class: 'marker'}, marker)
        ]
    }
    d.render(formated, el)
}

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

app.fetchPosts = async (page = 0, amount = 5) => {
    if (route.hash() == 'no-content') {
        if (!app.failedToFetchPosts) {
            location.hash = 'posts'
            route.handle()
            console.log('false alarm')
        } else {
            console.warn('not sure what happened')
        }
    }

    let writs
    if (d.isArr(app.postPages[page])) {
        postListView.innerHTML = ''
        let i = 0
        for (const wid of app.postPages[page]) {
            publicPost(app.posts[wid])
            if (++i >= amount) break
        }
    } else {
        try {
            writs = await app.writQuery({
                with_content: false,
                kind: 'post',
                amount,
                public: true,
                page
            })
            if (!d.isArr(writs)) {
                if (page == 0) {
                    mainView.innerHTML = ''
                    app.failedToFetchPosts = true
                    location.hash = 'no-content'
                    route.handle()
                }
                console.error(writs)
                throw new Error(writs.status)
            }
        } catch(e) {
            app.postPageForwardBtn.style.color = 'red'
            app.postPageForwardBtn.textContent = '404'
            setTimeout(() => {
                app.postPageForwardBtn.style.color = ''
                app.postPageForwardBtn.textContent = '>>'
            }, 3000)

            if (location.hash.includes('post:')) {
                if (page == 0) {
                    app.failedToFetchPosts = true
                    location.hash = 'no-content'
                    route.handle()
                } else {
                    location.hash = 'posts'
                    route.handle()
                }
            } else if (page == 0) {
                mainView.innerHTML = ''
                app.failedToFetchPosts = true
                location.hash = 'no-content'
                route.handle()
            }


            return
        }
        app.postPages[page] = []
        postListView.innerHTML = ''

        if (writs.length >= 5) {
            d.render(app.postPaginationView, contentDisplay)
        }

        writs.forEach(w => {
            w.page = page
            app.postPages[page].push(w.id)
            publicPost(app.posts[w.id] = w)
        })
    }
    app.emit.activePostPage(app.activePostPage = page)
    app.pageNumView.textContent = page
    app.cv('pageNot0', page != 0)
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

app.afterPostsInitialization = fn => app.postsInitialized ?
    (fn != null ? fn() : Promise.resolve(true)) : 
    fn != null ?
        app.once.postsInitialized(fn) :
        new Promise(app.once.postsInitialized)

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
            txt.updateInterval = setInterval(function update () {
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
    } catch(e) {
        console.error('app.voteWrit error: ', e)
    }
    return false
}