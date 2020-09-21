import app from '/js/site.min.js'
import route from '/js/router.min.js'
const d = app.d, df = d.domfn
const {div, h4, section, span, header} = df


const mainView = d.query('main[route-active]')
const contentDisplay = df.section({class: 'posts'})
const postDisplay = df.section({class: 'post'}, pd => {
    pd.parts = d.h /* html */ `
        <header class="post-header">
            <div>
                <h3 class="post-title" ref="title"></h3>
                <div class="author-name" ref="author"></div>
            </div>
            <div>
                <div class="posted" ref="date"></div>
                <div class="tags" ref="tags"></div>
            </div>
        </header>
    
        <article class="content" ref="content">
        </article>

        <aside class="post-comments" hidden ref="commentContainer">
            <header>Comments</header>
            <div ref="commentMaker"></div>
            <div ref="comments"></div>
        </aside>
    `.renderCollect(pd)
})


route('posts', contentDisplay)
if (location.hash == '' || location.hash == '#') location.hash = 'posts'

route('post', postDisplay)

const postNavView = d.html(/* html */`
    <nav class="post-nav">
        <button class="post-back-btn" onclick="location.hash = 'posts'">
            <- Go back to post list
        </button>
    </nav>
`)

route.on.post(hash => app.afterPostsInitialization(async () => {
    const post = app.activePost = app.posts[hash]
    const {title, tags, author, date, content, comments, commentsContainer} = postDisplay.parts
    title.textContent = post.title
    date.innerHTML = ''
    date.textContent = new Date(post.posted * 1000).toLocaleString()

    tags.innerHTML = ''
    post.tags.map(tag => df.span({$: tags, class:'tag'}, tag))
    author.textContent = 'By ' + post.author_name
    content.innerHTML = 'Content loading...'
    app.fetchPostContent(post.id).then(postContent => {
        content.innerHTML = ''
        d.render(d.html(postContent), content)
        setTimeout(() => {
            d.queryAll('.content code', content).forEach(el => {
                el.classList.add('language-rust')
            })
        }, 150)
    })

    df.prepend(mainView, postNavView)
}))
// TODO: pagination
app.view = {
    page: 0,
}

const publicPost = (w) => div({
    $: contentDisplay,
    class: 'post',
    attr: {pid: w.id},
    onclick(e, el) {
        location.hash = w.id
        app.fetchPostContent(w.id)
    }
},
    header(
        div(
            h4(w.title),
            div({class: 'author-name'}, `By ${w.author_name}`)
        ),
        div(
            div({class: 'posted'}, new Date(w.posted * 1000).toLocaleString()),
            div({class: 'tags'},
                w.tags.map(t => span({class: 'tag'}, t))
            )
        )
    )
);

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

app.writQuery({with_content: false, kind: 'post'}).then(writs => {
    if(!d.isArr(writs)) return console.error(writs)
    writs.forEach(w => {
        app.posts[w.id] = w
        publicPost(w)
    })
    app.postsInitialized = true
    app.emit.postsInitialized()
    document.head.append(
        df.link({
            attr: {
                rel: 'stylesheet',
                href: 'https://cdnjs.cloudflare.com/ajax/libs/prism/1.21.0/themes/prism-tomorrow.min.css'
            }
        })
    )
})

app.afterPostsInitialization = fn => {
    if (app.postsInitialized) fn()
    else app.once.postsInitialized(fn)
}