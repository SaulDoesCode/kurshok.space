import app from '/js/site.min.js'
const d = app.d, df = d.domfn

const wwLauncher = d.query('.ww-launcher')

const {
    wwView,
    titleInput,
    writingPad,
    tagInput,
    pushWritBtn,
    deleteWritBtn,
    writSelector,
    isPublicCheckbox,
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
            <button class="submit" ref="deleteWritBtn">Delete</button>
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


const writListEntry = (title, id) => d('div', {
    class: 'wl-entry',
    $: writList,
    attr: {wid: id}
},
    d('span', title)
)

app.user.writs = {}
app.ww = {}

app.pushWrit = async (title, raw_content, tags, opts = {}) => {
    const raw_writ = {
        title,
        raw_content: raw_content.trim(),
        tags,
        kind: 'post',
        public: true,
        viewable_by: []
    }
    Object.assign(raw_writ, opts)
    const res = await jsonPut('/writ', raw_writ)
    const data = await res.json()

    return data.ok ? Promise.resolve(data.data) : Promise.reject(data)
}

app.editableWritQuery({
    author_name: app.user.username,
    with_raw_content: true,
}).then(async writs => {
    console.log(writs)
    if (!d.isArr(writs)) {
        console.error("failed to fetch user's editable writs")
    }

    writs.forEach(w => {
        app.user.writs[w.id] = w
        writListEntry(w.title, w.id)
    })
})

d.on.click(writList, e => {
    if (e.target.classList.contains('selected')) return
    let wid = e.target.getAttribute('wid') || e.target.parentElement.getAttribute('wid')
    if (wid != null) {
        const writ = app.user.writs[wid]
        if (app.ww.selectedWLE) app.ww.selectedWLE.classList.remove('selected')
        app.ww.selectedWLE = d.query(`[wid="${wid}"]`)
        app.ww.selectedWLE.classList.add('selected')

        titleInput.value = writ.title
        writingPad.value = writ.raw_content
        tagInput.value = writ.tags.join(', ')
        console.log('got one:', writ)
    }
})