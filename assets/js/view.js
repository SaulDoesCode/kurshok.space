import app from '/js/site.min.js'
import route from '/js/router.min.js'
const d = app.d, df = d.domfn
const {div, h4, section, span, header} = df


const mainView = d.query('main[route-active]')
const contentDisplay = df.section({class: 'posts'})
app.postDisplay = df.section({class: 'full post'}, pd => {
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

        <aside class="post-comments" hidden ref="commentContainer">
            <header>Comments</header>
            <div ref="commentMaker"></div>
            <div ref="comments"></div>
        </aside>
    `.renderCollect(pd)
})


route('posts', contentDisplay)
if (location.hash == '' || location.hash == '#') location.hash = 'posts'

route('post', app.postDisplay)

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
           🡄 Back to Post List
        </button>
    </nav>
`)

route.on.post(async hash => {
    await app.afterPostsInitialization()
    const post = app.activePost = app.posts[hash]
    const {title, tags, author, date, content, comments, commentsContainer} = app.postDisplay.parts
    title.textContent = post.title
    date.innerHTML = ''
    d.render(app.renderUXTimestamp(post.posted), date)
    tags.innerHTML = ''
    post.tags.map(tag => df.span({$: tags, attr:{title: tag}, class:'tag'}, tag))
    author.textContent = 'By ' + post.author_name
    content.innerHTML = 'Content loading...'
    df.prepend(mainView, postNavView)
    const postContent = await app.fetchPostContent(post.id)
    content.innerHTML = ''
    if (post == app.activePost) {
        d.render(d.html(postContent), content)
        setTimeout(() => d.queryAll('.content code', content).forEach(el => {
            el.classList.add('language-rust')
        }), 60)
    }
    app.emit.postRendered(post, hash)
})

// TODO: pagination
app.view = {
    page: 0,
}

const publicPost = (w) => div({
    $: contentDisplay,
    class: 'post',
    attr: {pid: w.id},
    onclick(e, el) {
        if (e.target.className.includes('vote')) return
        app.fetchPostContent(w.id)
        location.hash = w.id
    }
},
    div({
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
                } catch(e) {
                    app.oneTimeAuthLauncher.on()
                }
                return
            }
            const isUp = e.target.classList.contains('up')
            const isDown = e.target.classList.contains('down')
            if (!isDown && !isUp) return
            e.target.classList.add('await-vote')
            const isSelected = e.target.classList.contains('selected')
            // unvote
            if (w.you_voted != null && isSelected) {
                const res = await app.voteWrit(w.id)
                if (res != false) {
                    el.downvote.classList.remove('selected')
                    el.upvote.classList.remove('selected')
                    e.target.classList.remove('await-vote')
                    app.formatVoteCount(el.voteCount, w.vote = res.data)
                    w.you_voted = null
                }
            } else if (isUp) {
                const res = await app.voteWrit(w.id, true)
                if (res != false) {
                    el.downvote.classList.remove('selected')
                    el.upvote.classList.add('selected')
                    e.target.classList.remove('await-vote')
                    app.formatVoteCount(el.voteCount, w.vote = res.data)
                    w.you_voted = true
                }
            } else if(isDown) {
                const res = await app.voteWrit(w.id, false)
                if (res != false) {
                    el.upvote.classList.remove('selected')
                    el.downvote.classList.add('selected')
                    e.target.classList.remove('await-vote')
                    app.formatVoteCount(el.voteCount, w.vote = res.data)
                    w.you_voted = false
                }
            }
        }
    },
        votesEl => [
            votesEl.upvote = span({
                class: {
                    up: true,
                    vote: true,
                    selected: w.you_voted === true,
                }
            }),
            votesEl.voteCount = span({class: 'vote-count'}, w.vote),
            votesEl.downvote = span({
                class: {
                    down: true,
                    vote: true,
                    selected: w.you_voted === false,
                }
            })
        ]
    ),
    header(
        div({class: 'title'},
            h4(w.title),
            div({class: 'author-name'}, `By ${w.author_name}`)
        ),
        div(
            div({class: 'posted'}, app.renderUXTimestamp(w.posted)),
            div({class: 'tags'},
                w.tags.map(t => {
                    const small = t.length > 11
                    return span({class: {tag: true, small}, attr: {title: t}}, t)
                })
            )
        )
    )
);

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
    if (data.ok) {
        return app.posts[id].content = data.data
    }
    throw new Error(data.status)
}

app.posts = Object.create(null)

app.writQuery({with_content: false, kind: 'post'}).then(async writs => {
    if(!d.isArr(writs)) return console.error(writs)
    writs.forEach(w => publicPost(app.posts[w.id] = w))
    app.emit.postsInitialized(app.postsInitialized = true)
    app.loadStyle('https://cdnjs.cloudflare.com/ajax/libs/prism/1.21.0/themes/prism-tomorrow.min.css', true)
    await import('/js/comments.min.js')
})

app.afterPostsInitialization = fn => app.postsInitialized ?
    (fn != null ? fn() : Promise.resolve(true)) : 
    fn != null ?
        app.once.postsInitialized(fn) :
        new Promise(resolve => app.once.postsInitialized(resolve))

app.dateFormat = 'HH:mm a DD MMM YYYY'

app.dayjsUXTSformat = ts => {
    const date = dayjs.unix(ts).utcOffset(2)
    return date.format(app.dateFormat) + ' | ' + date.fromNow()
}

app.renderUXTimestamp = (ts, formater = app.dayjsUXTSformat) => {
    const txt = d.txt()
    try {
        txt.textContent = formater(ts)
        txt.updateInterval = setInterval(() => {
            txt.textContent = formater(ts)
            if (!document.contains(txt)) clearInterval(txt.updateInterval)
        }, 60000)
    } catch (e) {
        txt.textContent = new Date(ts * 1000).toLocaleString()
        app.once.dayjsLoaded(() => app.renderUXTimestamp(ts, formater))
    }
    return txt
}

app.voteWrit = async (id, up) => {
    try {
        const res = await fetch(`/writ/${id}/${up == null ? 'unvote' : up ? 'upvote' : 'downvote'}`)
        return await res.json()
    } catch(e) {
        console.error('app.voteWrit error: ', e)
    }
    return false
}