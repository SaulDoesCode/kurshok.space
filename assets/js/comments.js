import app from '/js/site.min.js'
const d = app.d, df = d.domfn
const {div, h4, section, span, header} = df

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
    const res = await app.jsonPost('/comments', ops)
    console.log(res)
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
        cd.postBtn = df.button({
                class: 'cancel-btn',
            },
            'Cancel'
        ),
        cd.cancelBtn = df.button({
                class: 'post-btn',
            },
            'Post'
        )
    ),
    cd.commentList = div({
        class: 'comment-list',
    })
])

app.once.postRendered(async post => {
    await app.fetchComments(post.id)

})