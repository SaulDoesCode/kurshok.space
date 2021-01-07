import app from '/js/site.min.js'
const d = app.d, df = d.domfn
const {div, button, h4, section, span, header} = df
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

const commentsDisplay = app.commentsDisplay = section({
    $: location.pathname.includes('/post/') ? '.post' : app.postDisplay,
    class: 'comments'
}, cd => [
    header(
        cd.heading = h4({id: 'comments'}, 'Comments')
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
            cd.cancelBtn = button({
                class: 'cancel-btn',
                onclick(e) {
                    app.cancelCommentWriting()
                }
            },
                'Cancel'
            ),
            cd.postBtn = button({class: 'post-btn'},
                'Post'
            )
        )
    ),
    cd.list = div({
        class: 'comment-list',
    })
])

if (!app.user) {
    commentsDisplay.heading.setAttribute('title', 'make an account to comment')
}

app.gatherComment = () => {
    const {textarea, authorOnlyToggle} = commentsDisplay
    const raw_content = textarea.value.trim()
    const author_only = authorOnlyToggle.input.checked
    return {raw_content, author_only}
}

app.cancelCommentWriting = () => {
    commentsDisplay.textarea.value = ''
    if (commentsDisplay.authorOnlyToggle.input.checked) {
        commentsDisplay.authorOnlyToggle.input.checked = false
    }
    commentsDisplay.postBtn.textContent = 'Post'
    commentsDisplay.heading.textContent = 'Comments'
    app.editingComment = null
    app.replyingToComment = null
    if (app.editingCommentElement) {
        app.editingCommentElement.removeAttribute('hidden')
        app.editingCommentElement.style.position = ''
        app.editingCommentElement = null
    }
}

const commentPostHandler = d.once.click(commentsDisplay.postBtn, async e => {
    try {
        let res 
        if (commentsDisplay.classList.contains('edit-mode')) {
            app.editingComment.raw_content = commentsDisplay.textarea.value.trim()

            if (
                app.editingComment.raw_content == app.editingRawContent &&
                app.editingComment.author_only == commentsDisplay.authorOnlyToggle.input.checked
            ) {
                app.toast.error(`Comment you're editing is unchanged`)
                app.cancelCommentWriting()
                return
            }

            app.editingComment.author_only = commentsDisplay.authorOnlyToggle.input.checked

            res = await app.confirmCommentEdit(app.editingComment)
        } else {
            res = await app.makeComment(
                app.replyingToComment || ((app.postDisplay && app.activePostDisplay.id) || app.postID),
                (app.postDisplay && app.activePostDisplay.id) || app.postID
            )
            if (!res.ok) throw res.status || 'very bad, comment post failed miserably'
        }
        if (app.replyingToComment != null) {
            const cEl = commentsDisplay.list.querySelector(`[id="comment-${app.replyingToComment.replace('-', '/')}"]`)

            cEl.childenContainer = div({
                $: cEl,
                class: 'children'
            },
                app.formulateThread(res.data)
            )

            setTimeout(() => {
                cEl.childenContainer.focus()
                cEl.scrollIntoView({behavior: 'smooth'})
            }, 120)

            const btnRack = cEl.querySelector('.btn-rack')

            if (btnRack.querySelector('.hide-replies-btn') == null) {
                button.hide_replies_btn({
                    $: btnRack,
                    onclick(e, el) {
                        df.class(cEl, 'hidden-children')
                        el.textContent = (cEl.classList.contains('hidden-children') ? 'show' : 'hide') + ' replies'
                    }
                }, 'hide replies')
            }

        } else {
            let cEl
            if (app.editingCommentParent != null) {
                cEl = app.formulateThread(res.data, null, app.editingCommentParent)
            } else {
                cEl = app.formulateThread(res.data)
                commentsDisplay.list.prepend(cEl)
            }

            if (app.editingCommentElementChildren != null) {
                d.render(app.editingCommentElementChildren, cEl)
                app.editingCommentElementChildren = null
            }
        }
    } catch(e) {
        app.toast.error(`Commenting went wrong: ${e}`)
        console.error(e)
    } finally {
        commentsDisplay.textarea.value = ''
        if (commentsDisplay.authorOnlyToggle.input.checked) commentsDisplay.authorOnlyToggle.input.checked = false
        commentsDisplay.postBtn.textContent = 'Post'
        commentsDisplay.heading.textContent = 'Comments'
        app.replyingToComment = null
        app.editingCommentParent = null
        app.editingComment = null
        commentsDisplay.classList.remove('edit-mode')
        commentPostHandler.on()
    }
})

app.commentDateFormat = ts => {
    const date = dayjs.unix(ts).utcOffset(2)
    return '  ' + date.fromNow()
}

app.deleteComment = async cid => {
    cid = cid.replace('-', '/')
    var cEl;
    try {
        await d.queryAsync(`[id="comment-${cid}"]`)
        const delBtn = cEl.querySelector('span.delete')
        delBtn.classList.add('idle-animation')
    } catch(e) {}
    const res = await (await app.jsonDelete('/comment', cid)).json()
    if (res.ok) {
        app.toast.msg('Comment succesfully deleted')
        df.remove(cEl)
    }
    console.log(res)
}

app.editComment = async (cid, author_only) => {
    const cEl = await d.queryAsync(`[id="comment-${cid.replace('-', '/')}"]`)
    const res = await (await fetch(`/comment/${cid}/raw-content`)).json()
    if (!res.ok) {
        throw new Error('Could not retrieve comment raw_content')
    }
    commentsDisplay.textarea.value = res.data
    commentsDisplay.postBtn.textContent = 'Confirm Edit'
    commentsDisplay.heading.textContent = 'Comments: Editing'
    commentsDisplay.textarea.focus()
    commentsDisplay.authorOnlyToggle.input.checked = author_only
    commentsDisplay.classList.add('edit-mode')

    app.editingRawContent = res.data
    app.editingComment = {
        id: cid,
        writ_id: app.activePostDisplay.id,
        author_only,
    }

    if (cEl.parentElement.classList.contains('children')) {
        app.editingCommentParent = cEl.parentElement
    }

    cEl.setAttribute('hidden', '')
    cEl.style.position = 'fixed'
    app.editingCommentElement = cEl
}

app.confirmCommentEdit = async editingComment => {
    if (editingComment == null) throw new Error('Cannot edit a non-existent comment')
    
    const res = await (await app.jsonPost('/edit-comment', editingComment)).json()

    if (!res.ok) {
        app.toast.error('Failed to edit comment: ' + res.status)
        if (app.editingCommentElement) {
            app.editingCommentElement.removeAttribute('hidden')
            app.editingCommentElement.style.position = ''
            app.editingCommentElement = null
        }
        throw new Error(`app.confirmCommentEdit: ` + res.status || "it didn't work :(")
    }

    app.editingCommentElementChildren = app.editingCommentElement.querySelector('.children')

    df.remove(app.editingCommentElement)

    app.editingCommentElement = null

    app.toast.msg('Comment successfully edited')

    return res
}

if (location.pathname.includes('/post/')) {
    app.renderComments = async (post_id) => {
        const fcRes = await app.fetchComments(post_id)
        if (!fcRes.ok) return
        const commentTrees = fcRes.data
        const commentList = []

        for (const {comment, children} of commentTrees) {
            commentList.push(app.formulateThread(comment, children, commentsDisplay.list))
        }

        if (app.user == null || app.user.username == null) {
            app.d.run(() => {
                df.remove(commentsDisplay.commentWriter)
                if (commentList.length == 0) {
                    df.remove(commentsDisplay)
                    const wc = document.querySelector('.with-comments')
                    if (wc) {
                        wc.classList.remove('with-comments')
                    }
                }
            })
        }
    }
} else {
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
        const commentTrees = fcRes.data
        const commentList = []
    
        for (const {comment, children} of commentTrees) {
            commentList.push(app.formulateThread(comment, children, commentsDisplay.list))
        }
    
        if (app.user == null || app.user.username == null) {
            app.d.run(() => {
                df.remove(commentsDisplay.commentWriter)
                if (commentList.length == 0) {
                    df.remove(commentsDisplay)
                    const wc = document.querySelector('.with-comments')
                    if (wc) {
                        wc.classList.remove('with-comments')
                    }
                }
            })
        }
    })
}


const randHSLColor = () => `hsla(${Math.random()*360|0}, ${Math.random()*100|30}%, 50%, .8)`

app.formulateThread = (comment, children, $) => div.comment({
    $,
    css: {borderLeftColor: randHSLColor()},
    attr: {id: 'comment-' + comment.id},
}, cEl => [
    div({class: 'comment-content'},
        header(
            app.votesUI('comment', (() => (comment.id = comment.id.replace('/','-'), comment))()),
            span.author_name(comment.author_name),
            span.txt_divider(' - '),
            span.posted(app.renderUXTimestamp(comment.posted, app.commentDateFormat)),
            comment.edited != null && [span({class: 'line-divider'}, '|'), span({class: 'edited'},
                'edited',
                app.renderUXTimestamp(comment.edited, app.commentDateFormat)
            )],
            span.divider(),
            (app.user != null && comment.author_name == app.user.username) && [
                button.edit_btn({
                    onclick() {
                        app.editComment(comment.id, comment.author_only)
                    }
                }, 'edit'),
                span({
                    class: 'delete',
                    attr: {
                        title: 'click to delete your comment'
                    },
                    onclick(e) {
                        app.deleteComment(comment.id)
                    }
                }, app.dismissIcon())
            ]
        ),
        div({class: 'content'}, d.html(comment.content)),
        div({class: 'btn-rack'},
            button({
                class: 'reply-btn',
                onclick() {
                    app.replyingToComment = ('' + comment.id).replace('-', '/')
                    commentsDisplay.authorOnlyToggle.input.checked = comment.author_only
                    commentsDisplay.postBtn.textContent = 'Reply'
                    commentsDisplay.heading.textContent = 'Comments: Write a reply'
                    commentsDisplay.textarea.focus()
                }
            }, 'reply', btn => {
                if (!app.user) {
                    btn.setAttribute('title', 'make an account to reply')
                }
            }),
            
            children == null || children.length > 0 && button({
                class: 'hide-replies-btn',
                onclick(e, el) {
                    df.class(cEl, 'hidden-children')
                    el.textContent = cEl.classList.contains('hidden-children') ? 'show replies' : 'hide replies'
                }
            }, 'hide replies')
        )
    ),
    children == null || children.length > 0 && (cEl.childenContainer = div({class: 'children'},
        children.map(c => app.formulateThread(c.comment, c.children))
    ))
])