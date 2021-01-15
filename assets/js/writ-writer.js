import app from '/js/site.min.js'
const d = app.d, df = d.domfn
const {div, span, section, input, textarea} = df
const {query, on, once} = d

const wwLauncher = query('.ww-launcher')

const {
    wwView,
    titleInput,
    writingPad,
    tagInput,
    pushWritBtn,
    saveLocallyBtn,
    clearEditorBtn,
    writSelector,
    isPublicCheckbox,
    isCommentableCheckbox,
    writList
} = d.h/* html */`
<article class="writ-writer-view" ref="wwView">
    <section class="writer">
        <div>
            <input type="text" name="post-title" title="writ title" id="title-input" placeholder="writ title" autocomplete="off" ref="titleInput">
        </div>
        <div class="writing-pad-container">
            <textarea class="writing-pad" ref="writingPad" title="writ content" spellcheck="true" minlength="10" required placeholder="content of your writ (it can be markdown)"></textarea>
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
            <button class="submit" ref="saveLocallyBtn">Save locally</button>
            <button class="submit" ref="clearEditorBtn">Clear Editor</button>
        </section>
    </section>
    <aside class="writ-selector" ref="writSelector">
        <header>Your Writs</header>
        <section class="writ-list" ref="writList"></section>
    </aside>
</article>`.collect()

app.writListPaginationView = section.pagination(
    app.writListPageBackBtn = div.page_back({
            contingentVisibility: 'wlpageNot0',
            onclick(e) {
                app.populateWritList({back: true})
            }
        },
        '<<'
    ),
    app.writListPageNumView = div.page_num(),
    app.writListPageForwardBtn = div.page_forward({
            onclick(e) {
                app.populateWritList({next: true})
            }
        },
        '>>'
    ),
)

writList.after(app.writListPaginationView)

writingPad.value = ''

app.titleInput = titleInput
app.writingPad = writingPad
app.tagInput = tagInput

app.doob = async (doobs = 10) => {
    while (doobs-- > 1) {
        app.clearEditor()
        app.titleInput.value = `Doob # ${x}`
        app.writingPad.value = `Doob # ${x}`
        app.tagInput.value = `test`
        await app.editorPushWrit()
    }
}

;(app.wwTS = app.setupToggleSituation(wwLauncher, wwView, 'body', {
    viewOutAnimation: 'fade-out 220ms ease-out',
    delayRemoveMS: 220,
    background: true,
})).toggleView()

const writListEntry = (title, id, prepend = false) => div({
    class: {
        'wl-entry': true,
        unpushed: id == null
    },
    $: prepend ? undefined : writList,
    attr: {wid: id == null ? title : id}
}, parent => {
    if (prepend) writList.prepend(parent)
    return [
        parent.titleSpan = span(title),
        div(
            () => {
                const delBtn = span({
                    class: 'delete-writ',
                    attr: {title: 'Double click/tap to delete writ'}
                }, '🗑')

                // manually jigging double click/tap
                let timeout, clicks = 0
                const clickHandler = on.pointerup(delBtn, async e => {
                    clearTimeout(timeout)
                    if (++clicks == 2) {
                        clicks = 0
                    } else {
                        if (clicks == 1) delBtn.classList.add('prep')
                        timeout = setTimeout(() => {
                            delBtn.classList.remove('prep')
                            clicks = 0
                        }, 900)
                        return
                    }
                    try {
                        clickHandler.off()
                        if (id != null) {
                            const res = await app.deleteWritRequest(id)
                            if (res.ok) {
                                df.remove(parent)
                                if (app.ww.active && app.ww.active.id == id) app.clearEditor()
                                delete app.ww.writs[id]
                                app.emit('postEdit:' + id, false)
                            }
                        } else {
                            await localforage.removeItem('unpushed:' + title)
                            delete app.ww.unpushed[title]
                            df.remove(parent)
                            if (app.ww.active && app.ww.active.title == title) app.clearEditor()
                        }
                    } catch (e) {
                        clickHandler.on()
                        console.error(`Well, that didn't work: ${e}`)
                    }
                })

                return delBtn
            }
        )
    ]
})

app.ww = {writs: {}, unpushed: {}}

app.pushWrit = async (title, raw_content, tags, ops = {}) => {
    if (tags.length === 1 && tags[0] === "") {
        throw new Error('posts need at least one tag')
    }

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

    if (data.ok) {
        if (raw_writ.id != null) {
            const wle = writList.querySelector(`[wid="${raw_writ.id}"]`)
            if (wle) {
                app.editableWritQuery({
                    author_name: app.user.username,
                    with_raw_content: false,
                    ids: [raw_writ.id],
                }).then(async writs => {
                    if (!d.isArr(writs)) {
                        app.toast.error('Failed to fetch your editable writs')
                        return console.error("failed to fetch user's editable writs")
                    }
                    for (const w of writs) {
                        w.raw_content = raw_writ.raw_content
                        app.ww.writs[w.id] = w
                        wle.titleSpan.textContent = w.title
                        app.emit('postEdit:' + w.id, w)
                    }
                })
            }
        } else {
            raw_writ.id = data.data.id
            raw_writ.posted = data.data.posted
            raw_writ.slug = data.data.slug
            app.emit.newPost(raw_writ.id)
            app.ww.writs[raw_writ.id] = raw_writ
            writListEntry(raw_writ.title, raw_writ.id)
            app.clearEditor()
        }
        return Promise.resolve(data.data)
    }
    return Promise.reject(data)
}

app.deleteWritRequest = writID => app.txtDelete('/writ', writID)

app.wwQuery = {
    author_name: app.user.username,
    with_raw_content: false,
    page: 0,
}

app.populateWritList = ({page = app.wwQuery.page, next, back} = {}) => {
    if (back === true) {
        if (app.wwQuery.page == 0) return
        app.wwQuery.page--
    } else if (next === true) {
        app.wwQuery.page++
    } else if (page != app.wwQuery.page) {
        app.wwQuery.page = page
    }
    app.editableWritQuery(app.wwQuery).then(async writs => {
        if (!d.isArr(writs)) {
            app.toast.error('Failed to fetch your editable writs')
            return console.error("failed to fetch user's editable writs")
        }

        app.cv('wlpageNot0', app.wwQuery.page != 0)
        app.writListPageNumView.textContent = app.wwQuery.page

        writList.innerHTML = ''
        for (const w of writs) {
            app.ww.writs[w.id] = w
            writListEntry(w.title, w.id)
        }

        if (app.wwQuery.page == 0) {
            for (const key of await (await app.localForage()).keys()) {
                if (key.includes('unpushed:')) {
                    const uw = JSON.parse(await localforage.getItem(key))
                    app.ww.unpushed[uw.title] = uw
                    writListEntry(uw.title)
                }
            }
        }
    })
}

app.populateWritList()


app.rawContentRequest = async wid => {
    const res = await fetch('/writ-raw-content/' + wid)
    return await res.json()
}

app.localForage = () => app.localForageLoaded ?
    Promise.resolve(window.localforage) :
    new Promise(resolve => app.once.localForageLoaded(() => resolve(window.localforage)))

d.run(async () => {
    await app.loadScriptsThenRunSequentially(true, '/js/localforage.min.js')
    app.emit('localForageLoaded', app.localForageLoaded = true)
})

on.pointerup(writList, e => {
    if (e.target.classList.contains('selected') || e.target.parentElement.classList.contains('selected')) return
    let wid = e.target.getAttribute('wid') || e.target.parentElement.getAttribute('wid')
    if (wid != null) {
        const writ = app.ww.active = (app.ww.writs[wid] || app.ww.unpushed[wid])

        if (app.ww.writs[wid] == null) {
            pushWritBtn.after(saveLocallyBtn)
        } else {
            df.remove(saveLocallyBtn)
        }

        if (app.ww.selectedWLE) app.ww.selectedWLE.classList.remove('selected')
        app.ww.selectedWLE = d.query(`[wid="${wid}"]`)
        app.ww.selectedWLE.classList.add('selected')

        let noContent = writ.raw_content == null
        if (writ.raw_content == null) {
            app.rawContentRequest(writ.id).then(data => {
                if (!data.ok) {
                    writingPad.value = 'Ok, so loading failed for some reason, you should reload or something, it\'s probably a client side error, or less likely, a database issue - ' + (data.status || '')
                }
                writingPad.value = writ.raw_content = data.data
                noContent = false
            })
        }

        titleInput.value = writ.title
        writingPad.value = writ.raw_content || 'Hang tight, the content is loading...'
        setTimeout(() => {
            if (noContent) {
                let tick = 0
                const baseMsg = 'Hang tight, the content is loading'
                let interval = setInterval(() => {
                    if (noContent) {
                        let dots = ''
                        d.each(tick++, () => dots += '.')
                        writingPad.value = baseMsg + dots
                        if (tick == 4) tick = 0
                    } else {
                        clearInterval(interval)
                        tick = 0
                        writingPad.value = writ.raw_content
                    }
                }, 220);
            }
        }, 220)
        tagInput.value = writ.tags.join(', ')
        isPublicCheckbox.checked = writ.public
        isCommentableCheckbox.checked = writ.commentable
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
    pushWritBtn.after(saveLocallyBtn)
}

app.gatherWritFromWriter = () => {
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

    return {title, raw_content, tags, ops}
}

app.editorPushWrit = async () => {
    console.log('trying to push writ...')
    let res

    const writFields = app.gatherWritFromWriter()
    if (app.ww.active) writFields.ops.id = app.ww.active.id
    res = await app.pushWrit(
        writFields.title,
        writFields.raw_content,
        writFields.tags,
        writFields.ops
    )

    if (res && res.title != null) {
        app.toast.msg(`success, writ posted: ${res.title}`)
    }
    return res
}

on.pointerup(saveLocallyBtn, async e => {
    if (app.ww.active && app.ww.active.id != null) return
    const {title, raw_content, tags, ops} = app.gatherWritFromWriter()
    const writ = {title, raw_content, tags, ...ops}
    await localforage.setItem('unpushed:' + title, JSON.stringify(writ))
    app.ww.active = app.ww.unpushed[title] = writ

    if (app.ww.selectedWLE) app.ww.selectedWLE.classList.remove('selected')
    app.ww.selectedWLE = writListEntry(title)
    app.ww.selectedWLE.classList.add('selected')
})

on.pointerup(clearEditorBtn, app.clearEditor)

on.pointerup(pushWritBtn, e => {
    app.editorPushWrit()
})

on.input(tagInput, app.filterTagInput)
on.keydown(tagInput, e => {
    if (e.key === 'Enter') {
        app.editorPushWrit()
        e.preventDefault()
        titleInput.focus()
    } else if (e.ctrlKey) {
        setTimeout(() => {
            tagInput.value = app.cleanseTagValue(tagInput.value)
        }, 0)
    }
})

on.input(titleInput, e => {
    if (e.key === 'Enter') {
        writingPad.focus()
        e.preventDefault()
    } else if (app.ww.active && app.ww.selectedWLE) {
        app.ww.selectedWLE.titleSpan.textContent = titleInput.value
    }
})
