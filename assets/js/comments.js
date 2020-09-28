import app from '/js/site.min.js'
const d = app.d, df = d.domfn
const {div, h4, section, span, header} = df
const {toggleBox} = app.components

app.fetchComments = async (wid, ops = {}) => {
    if (ops.path == null) ops.path = wid + '/'
    if (ops.page == null) ops.page = 1
    /*
        ops.ids = null
        ops.skip_ids = null
        ops.authors = null
        ops.author_ids = null
        ops.public = null
        ops.is_admin = null
        ops.requestor_id = null
        ops.author_name = null
        ops.author_handle = null
        ops.author_id = null
        ops.exluded_author_ids = null
        ops.posted_before = null
        ops.posted_after = null
        ops.year = null
        ops.month = null
        ops.day = null
        ops.hour = null
        ops.max_level = null
        ops.amount = null
    */
    try {
        const res = await app.jsonPost('/comments', ops)
        return await res.json()
    } catch(e) {
        return {ok: false}
    }
}

app.makeComment = async (
    parent_id,
    writ_id,
    comment = app.gatherComment()
) => {
    if (comment.public == null) comment.public = true
    if (comment.author_only == null) comment.author_only = false

    if (parent_id == null || parent_id.length < 2) {
        throw new Error('app.makeComment: invalid parent id')
    }
    if (writ_id == null) writ_id = parent_id
    if (writ_id.length < 2) {
        throw new Error('app.makeComment: invalid writ id')
    }
    if (comment.raw_content == null || comment.raw_content.length < 2) {
        throw new Error(`app.makeComment: invalid comment content, it's either too long or too short`)
    }
    const res = await app.jsonPut('/comment', {
        parent_id,
        writ_id,
        public: comment.public,
        raw_content: comment.raw_content,
        author_only: comment.author_only
    })

    return await res.json()
}

const commentsDisplay = section({
    $: app.postDisplay,
    class: 'comments'
}, cd => [
    header(
        h4('Comments')
    ),
    cd.commentWriter = div({
        class: 'comment-writer',
    },
        cd.textarea = df.textarea({
            placeholder: 'write a comment'
        }),
        div({class: 'comment-controls'},
            div({class: 'togglebox-container'},
                df.label({attr: {for: 'author-only'}}, 'Author Only'),
                cd.authorOnlyToggle = toggleBox('author-only', {
                    id: 'author-only',
                    attr: {
                        title: 'check this to make your comment visible only to the post author and no one else'
                    }
                })
            ),
            cd.cancelBtn = df.button({
                class: 'cancel-btn',
                onclick(e) {
                    cd.textarea.value = ''
                    if (cd.authorOnlyToggle.input.checked) cd.authorOnlyToggle.input.checked = false
                }
            },
                'Cancel'
            ),
            cd.postBtn = df.button({
                    class: 'post-btn',
            },
                'Post'
            )
        )
    ),
    cd.list = div({
        class: 'comment-list',
    })
])

app.gatherComment = () => {
    const {textarea, authorOnlyToggle} = commentsDisplay
    const raw_content = textarea.value.trim()
    const author_only = authorOnlyToggle.input.checked
    return {raw_content, author_only}
}

const commentPostHandler = d.once.click(commentsDisplay.postBtn, async e => {
    try {
        const res = await app.makeComment(app.activePostDisplay.id)
        if (!res.ok) throw res.status || 'very bad, comment post failed miserably'
        commentsDisplay.textarea.value = ''
        if (commentsDisplay.authorOnlyToggle.input.checked) commentsDisplay.authorOnlyToggle.input.checked = false
        commentsDisplay.list.prepend(app.formulateThread(res.data))
    } catch(e) {
        console.error(e)
    } finally {
        commentPostHandler.on()
    }
    // await app.fetchComments(app.activePostDisplay.id, {ids: []})
})

app.commentDateFormat = ts => {
    const date = dayjs.unix(ts).utcOffset(2)
    return '  ' + date.fromNow()
}

app.on.postRendered(async post => {
    app.activePostDisplay = post

    app.postDisplay.classList.toggle('with-comments', post.commentable)

    if (post.commentable) {
        commentsDisplay.list.innerHTML = ''
        d.render(commentsDisplay, app.postDisplay)
    } else {
        df.remove(commentsDisplay)
        return
    }

    const fcRes = await app.fetchComments(post.id)
    if (!fcRes.ok) return
    console.log(fcRes)
    const commentTrees = fcRes.data
    const commentList = []

    for (const {comment, children} of commentTrees) {
        commentList.push(app.formulateThread(comment, children))
    }
    d.render(commentList, commentsDisplay.list)
})

app.formulateThread = (comment, children) => div({class: 'comment'},
    header(
        span({class: 'author-name'}, comment.author_name),
        span({class: 'txt-divider'}, ' - '),
        span({class: 'posted'}, 
            app.renderUXTimestamp(comment.posted, app.commentDateFormat)
        )
    ),
    div({class: 'content'}, d.html(comment.content)),
    children == null || children.length > 0 && div({class: 'children'},
        children.map(({cmnt, chrn}) => formulateComment(cmnt, chrn))
    )
)