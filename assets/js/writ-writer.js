import app from '/js/site.min.js'
const d = app.d, df = d.domfn
const {div, span} = df

const wwLauncher = d.query('.ww-launcher')

const {
    wwView,
    titleInput,
    writingPad,
    tagInput,
    pushWritBtn,
    clearEditorBtn,
    writSelector,
    isPublicCheckbox,
    isCommentableCheckbox,
    writList
} = d.h `
<article class="writ-writer-view" ref="wwView">
    <section class="writer">
        <div>
            <input type="text" name="post-title" title="writ title" id="title-input" placeholder="writ title" autocomplete="off" ref="titleInput">
        </div>
        <div>
            <textarea class="writing-pad" ref="writingPad" title="writ content" spellcheck="true" wrap="off" minlength="10" required placeholder="content of your writ (it can be markdown)"></textarea>
        </div>
        <div class="tags-and-toggles">
            <input type="text" name="tags" title="tag input" id="tag-input" placeholder="comma, separated, tags" autocomplete="off" ref="tagInput">
            <div>
                <label for="is-public">public</label>
                <div class="togglebox"> 
                    <input type="checkbox" name="public" id="is-public" ref="isPublicCheckbox" checked>
                    <span></span>
                </div>
            </div>
            <div>
                <label for="is-commentable">commentable</label>
                <div class="togglebox"> 
                    <input type="checkbox" name="commentable" id="is-commentable" ref="isCommentableCheckbox" checked>
                    <span></span>
                </div>
            </div>
        </div>
        <section class="ribbon">
            <button class="submit" ref="pushWritBtn">Push</button>
            <button class="submit" ref="clearEditorBtn">Clear Editor</button>
        </section>
    </section>
    <aside class="writ-selector" ref="writSelector">
        <header>Writs</header>
        <section class="writ-list" ref="writList"></section>
    </aside>
</article>`.collect()

writingPad.value = ''

;(app.wwTS = app.setupToggleSituation(wwLauncher, wwView, 'body', {
    viewOutAnimation: 'fade-out 220ms ease-out',
    delayRemoveMS: 220,
})).toggleView()


const writListEntry = (title, id) => div({
    class: 'wl-entry',
    $: writList,
    attr: {wid: id}
},
    span(title),
    div(
        span({
            class: 'delete-writ',
            once: {
                async pointerup() {
                    try {
                        const res = await app.deleteWrit(app.ww.writs[id])
                        if (res.ok) {
                            df.remove(d.query(`[wid="${id}"]`))
                            if (app.ww.active && app.ww.active.id == id) app.clearEditor()
                            delete app.ww[id]
                        }
                    } catch(e) {
                        console.error(`Well, that didn't work: ${e}`)
                    }
                }
            }
        }, 'Del')
    )
)

app.ww = {writs: {}}

app.pushWrit = async (title, raw_content, tags, ops = {}) => {
    const raw_writ = {
        title,
        raw_content: raw_content.trim(),
        tags,
        kind: 'post',
        public: true,
        viewable_by: [],
        ...ops,
    }
    const res = await app.jsonPut('/writ', raw_writ)
    const data = await res.json()

    return data.ok ? Promise.resolve(data.data) : Promise.reject(data)
}

app.deleteWrit = rawWrit => app.jsonDelete('/writ', rawWrit)

app.editableWritQuery({
    author_name: app.user.username,
    with_raw_content: true,
}).then(async writs => {
    if (!d.isArr(writs)) {
        console.error("failed to fetch user's editable writs")
    }
    console.log(writs)

    for (const w of writs) {
        app.ww.writs[w.id] = w
        writListEntry(w.title, w.id)
    }
})

d.on.pointerup(writList, e => {
    if (e.target.classList.contains('selected') || e.target.parentElement.classList.contains('selected')) return
    let wid = e.target.getAttribute('wid') || e.target.parentElement.getAttribute('wid')
    if (wid != null) {
        const writ = app.ww.active = app.ww.writs[wid]
        if (app.ww.selectedWLE) app.ww.selectedWLE.classList.remove('selected')
        app.ww.selectedWLE = d.query(`[wid="${wid}"]`)
        app.ww.selectedWLE.classList.add('selected')

        titleInput.value = writ.title
        writingPad.value = writ.raw_content
        tagInput.value = writ.tags.join(', ')
        isPublicCheckbox.checked = writ.commentable
        isCommentableCheckbox.checked = writ.public
        pushWritBtn.textContent = 'Update'
    }
})

app.clearEditor = () => {
    if (app.ww.active) app.ww.active = null
    titleInput.value = writingPad.value = tagInput.value = ''
    isPublicCheckbox.checked = isCommentableCheckbox.checked = true
    if (app.ww.selectedWLE) {
        app.ww.selectedWLE.classList.remove('selected')
        app.ww.selectedWLE = null
        pushWritBtn.textContent = 'Push'
    }
}
app.editorPushWrit = async () => {
    console.log('trying to push writ...')
    let res

    const title = titleInput.value.trim()
    const raw_content = writingPad.value.trim()
    const public = isPublicCheckbox.checked
    const commentable = isCommentableCheckbox.checked
    const tags = tagInput.value.split(',').map(t => t.trim())
    const ops = {
        is_md: true,
        public,
        commentable
    }
    if (app.ww.active) ops.id = app.ww.active.id
    res = await app.pushWrit(title, raw_content, tags, ops)

    if (res != null && res.ok) {
        console.log(res)
        return res
    }
}

d.on.pointerup(clearEditorBtn, app.clearEditor)

d.on.pointerup(pushWritBtn, e => {
    app.editorPushWrit()
})
