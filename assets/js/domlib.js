export default (d => {
  d.isArr = Array.isArray

  d.isNil = o => o == null

  d.isDef = o => o != null

  d.isFunc = o => o instanceof Function

  d.isBool = o => typeof o === 'boolean'

  d.isObj = o => o != null && o.constructor === Object

  d.isStr = o => typeof o === 'string'

  d.isNum = o => typeof o === 'number' && !isNaN(o)

  d.isInt = o => d.isNum(o) && o % 1 === 0

  d.isArrlike = o => o != null && (d.isArr(o) || (!(o instanceof Function || o instanceof Node) && o.length % 1 === 0))

  d.isNode = o => o instanceof Node

  d.isNodeList = (o, arr = true) => o instanceof NodeList || (arr && d.allare(o, d.isNode))

  d.isPrimitive = o => (o = typeof o, o === 'string' || o === 'number' || o === 'boolean')

  d.isEl = o => o instanceof Element

  d.isPromise = o => typeof o === 'object' && d.isFunc(o.then)

  d.isRegExp = o => o instanceof RegExp

  d.isEmpty = o => d.isNil(o) || !((d.isObj(o) ? Object.keys(o) : o).length || o.size)

  d.isMounted = (child, parent = document) => d.isNodeList(child) ? Array.from(child).every(n => d.isMounted(n)) : parent === child || !!(parent.compareDocumentPosition(child) & 16)

  d.isSvg = o => o instanceof SVGElement

  d.isInput = (o, contentEditable) => o instanceof HTMLInputElement || o instanceof HTMLTextAreaElement || (
    !!contentEditable && o instanceof Element && o.getAttribute('contenteditable') === 'true'
  )

  d.isRenderable = o => o instanceof Node || d.isPrimitive(o) || d.allare(o, d.isRenderable)

  /*
   * allare checks whether all items in an array are like a given param
   * it's similar to array.includes but allows functions
   */
  d.allare = (arr, like) => {
    if (!d.isArrlike(arr)) return false
    const isfn = like instanceof Function
    for (let i = 0; i < arr.length; i++) {
      if (!(isfn ? like(arr[i]) : arr[i] === like)) {
        return false
      }
    }
    return true
  }

  /*
   * compose a typical function composition functions
   * @ example compose(x => x + 1, x => x + 1)(1) === 3
   */
  d.compose = (...fns) => fns.reduce((a, b) => (...args) => a(b(...args)))

  /*
   * curry a function
   * and optionally
   * set the arity or pre bound arguments
   */
  d.curry = (fn, arity = fn.length, ...args) => arity <= args.length ? fn(...args) : d.curry.bind(null, fn, arity, ...args)

  d.assign = Object.assign

  d.clone = (host, empty) => d.assign(empty ? Object.create(null) : {}, host)

  /*
   * flatten recursively spreads out nested arrays
   * to make the entire array one dimentional
   * @example flatten([1, [2, [3]], 4, [5]]) -> [1, 2, 3, 4, 5]
   * @example flatten(x) -> [x]
   */
  d.flatten = (arr, result = [], encaptulate = true) => {
    if (encaptulate && !d.isArr(arr)) return [arr]
    for (let i = 0; i < arr.length; i++)
      d.isArr(arr[i]) ? d.flatten(arr[i], result) : d.result.push(arr[i])
    return result
  }

  /*
   * runAsync runs a function asynchronously
   */
  d.runAsync = window.requestIdleCallback ? (fn, ...args) => window.requestIdleCallback(fn.bind(undefined, ...args)) : (fn, ...args) => setTimeout(fn, 0, ...args)

  /*
   * run runs a function on DOMContentLoaded or asynchronously
   * if document.body is present and loaded
   */
  d.run = function () {
    document.body || document.readyState === 'complete' ?
      d.runAsync.apply(undefined, arguments) :
      window.addEventListener('DOMContentLoaded', e => {
        d.runAsync.apply(undefined, arguments)
      }, {once: true})
  }

  d.html = (input, ...args) => {
    if (args.length > 2) return d.h(input, ...args)
    if (input instanceof Function) input = input(...args)
    if (input instanceof Promise) return new Promise(resolve => {
      input.then(i => resolve(d.html(i, ...args)))
    })
    if (input instanceof Node) return input
    if (d.isNum(input)) input = String(input)
    if (typeof input === 'string') return Array.from(document.createRange().createContextualFragment(input).childNodes)
    if (d.isArr(input)) return input.map(i => d.html(i, ...args))
    throw new Error('.html: unrenderable input')
  }

  d.frag = inner => inner != null ? d.html(inner) : document.createDocumentFragment()

  /*
   * DOM Query Selector Functions
   */
  d.query = (selector, host = document) => d.isNode(selector) ? selector : d.query(host).querySelector(selector)

  d.queryAsync = (selector, host) => new Promise((resolve, reject) => {
    const find = () => {
      const result = d.query(selector, host)
      result == null ?
        reject(new Error("queryAsync: couldn't find " + selector)) :
        resolve(result)
    }
    document.body ? find() : d.run(find)
  })

  /*
   *  queryAll(selector String|Node, host = document String|Node)
   *  it returns an array of elements matching a selector,
   *  a nicer querySelectorAll essentially.
   */
  d.queryAll = (selector, host = document) => Array.from(d.query(host).querySelectorAll(selector))

  d.queryEach = (selector, fn, host = document) => {
    if (!d.isFunc(fn))[fn, host] = [host, document]
    return d.each(d.queryAll(selector, host), fn)
  }

  /*
   * each iterates over arrays, objects, integers,
   * and anything implementing .forEach
   */
  d.each = (iterable, fn) => {
    if (iterable != null) {
      if (d.isObj(iterable)) {
        for (const key in iterable) fn(iterable[key], key, iterable)
      } else if (iterable.length) {
        const len = iterable.length
        let i = 0
        while (i !== len) fn(iterable[i], i++, iterable)
      } else if (iterable.forEach) {
        iterable.forEach(fn)
      } else if (d.isInt(iterable)) {
        let i = 0
        while (i < iterable) fn(i++, iterable)
      }
    }
    return iterable
  }

  /*
   * infinify takes a function that has a string (like an event type or key)
   * and returns a proxy which binds the key of any get operation
   * as that initial string argument enabling a very natural feeling API
   * @scope infinify(func) -> Proxy<func>
   * @example const emit = infinify(emitFN); emit.anyEvent(details)
   */
  d.infinify = (fn, reflect) => new Proxy(fn, {
    get: reflect === true ?
      (fn, key) => key in fn ? Reflect.get(fn, key) : fn.bind(null, key) : (fn, key) => fn.bind(null, key)
  })

  d.copyprop = (host, obj, key) => (Object.defineProperty(host, key, Object.getOwnPropertyDescriptor(obj, key)), host)

  /*
   * merge(host Object|Array, target Object|Array)
   * merge objects together deeply.
   * it copies prop descriptions instead of raw values.
   */
  d.merge = (host, target) => {
    if (d.isArr(host) && d.isArr(target)) {
      for (const val of target)
        if (!host.includes(val)) host.push(val)
    } else if (d.merge.able(host) && d.merge.able(target)) {
      for (const key in target) {
        if (key in host) {
          const old = host[key]
          const val = target[key]
          if (d.merge.able(old) && d.merge.able(val)) {
            d.merge(old, val)
          } else if (val != null) {
            d.copyprop(host, target, key)
          }
        } else {
          d.copyprop(host, target, key)
        }
      }
    }
    return host
  }

  d.merge.able = o => d.isArr(o) || (o != null && typeof o === 'object' && !d.isFunc(o.then))

  d.emitter = (host = Object.create(null), listeners = new Map()) => Object.assign(host, {
    emit: d.infinify((event, ...data) => d.runAsync(() => {
      if (listeners.has(event))
        for (const h of listeners.get(event)) h.apply(null, data)
    })),
    on: d.infinify((event, handler) => {
      if (!listeners.has(event)) listeners.set(event, new Set())
      listeners.get(event).add(handler)
      const manager = () => host.off(event, handler)
      manager.off = manager
      manager.on = () => (manager(), host.on(event, handler))
      manager.once = () => (manager(), host.once(event, handler))
      return manager
    }),
    once: d.infinify((event, handler) => host.on(event, function h() {
      handler(...arguments)
      host.off(event, h)
    })),
    off: d.infinify((event, handler) => {
      if (listeners.has(event)) {
        const ls = listeners.get(event)
        ls.delete(handler)
        if (!ls.size || handler == null) listeners.delete(event)
      }
    }),
    clear: () => (listeners.clear(), host)
  })

  const listen = function (once, target, type, fn, options = false) {
    if (d.isStr(target)) target = d.queryAll(target)
    if ((d.isArr(target) || target instanceof NodeList) && target.length === 1) {
      target = target[0]
    }
    if (!target || (d.isArr(target) && !target.length) || !target.addEventListener) {
      throw new Error('nil/empty event target(s)')
    }

    let typeobj = d.isObj(type)
    if (type == null || !(typeobj || d.isStr(type))) {
      throw new TypeError('cannot listen to nil or invalid event type')
    }

    if (d.isArr(target)) {
      for (let i = 0; i < target.length; i++) {
        target[i] = listen(once, target[i], typeobj ? d.clone(type) : type, fn, options)
      }
      target.off = () => {
        for (const h of target) h()
        return target
      }
      target.on = mode => {
        for (const h of target) h.on(mode)
        return target
      }
      return target
    }

    if (typeobj) {
      for (const name in type) {
        type[name] = listen(once, target, name, type[name], options)
      }
      target.off = () => {
        for (const h of Object.values(type)) h()
        return target
      }
      target.on = mode => {
        for (const h of Object.values(type)) h.on(mode)
        return target
      }
      return type
    }

    let wrapper
    if (typeof fn === 'string' && options instanceof Function) {
      let matcher = fn
      fn = options
      options = arguments[5]
      if (options == null) options = false
      wrapper = function (event) {
        if (
          event.target != null &&
          event.target !== this &&
          event.target.matches(matcher)
        ) {
          fn.call(this, event, target)
          if (off.once) off()
        }
      }
    } else {
      wrapper = function (event) {
        fn.call(this, event, target)
        if (off.once) off()
      }
    }

    const on = mode => {
      if (mode != null && mode !== off.once) off.once = !!mode
      target.addEventListener(type, wrapper, options)
      off.ison = true
      return off
    }

    const off = d.assign(() => {
      target.removeEventListener(type, wrapper)
      off.ison = false
      return off
    }, {
      target,
      on,
      once
    })
    off.off = off

    return on()
  }

  const infinifyListen = {
    get: (ln, type) => (tgt, fn, opts) => ln(tgt, type, fn, opts)
  }

  d.on = new Proxy(listen.bind(null, false), infinifyListen)
  d.once = new Proxy(listen.bind(null, true), infinifyListen)
  d.EventManager = d.curry(listen, 3)

  // vpend - virtual append, add nodes and append them as a document fragment
  d.vpend = (
    children,
    host,
    connector = 'appendChild',
    dfrag = d.frag(),
    noHostAppend
  ) => {
    for (let i = 0; i < children.length; i++) {
      let child = children[i]
      if (child instanceof Function) {
        if ((child = child(host)) === host) continue
        else if (child instanceof Function) {
          let lvl = 0
          let ishost = false
          let lastchild
          while (child instanceof Function && lvl < 25) {
            lastchild = child
            child = child()
            if ((ishost = child === host) || lastchild === child) break
            lvl++
          }
          if (ishost) continue
        }
      }

      if (child == null) continue
      const ctr = child.constructor
      if (ctr === String || ctr === Number) {
        if (!child.length) continue
        child = new Text(child)
      } else if (d.isArr(child)) {
        child = d.vpend(child, host, connector, dfrag, true)
      }

      if (child instanceof Node) {
        dfrag.appendChild(child)
        children[i] = child
      }
    }
    if (host && !noHostAppend) d.run(() => host[connector](dfrag))
    return children
  }

  /*
   * prime takes an array of renderable entities
   * and turns them into just nodes and functions
   * (to be unwrapped/degloved later rather than sooner [by vpend])
   */
  d.prime = (...nodes) => {
    for (let i = 0; i < nodes.length; i++) {
      let n = nodes[i]
      const ntype = typeof n
      if (n == null || ntype === 'boolean') {
        nodes.splice(i, 1)
        continue
      }
      if (n instanceof Node || n instanceof Function) {
        continue
      } else if (ntype === 'string' || ntype === 'number') {
        const nextI = i + 1
        if (nextI < nodes.length) {
          const next = nodes[nextI]
          const nexttype = typeof next
          if (nexttype === 'string' || nexttype === 'number') {
            nodes[i] = String(n) + String(next)
            nodes.splice(nextI, 1)
            i--
          }
        } else {
          nodes[i] = new Text(String(n))
        }
        continue
      }

      const isnl = n instanceof NodeList
      if (isnl) {
        if (n.length < 2) {
          nodes[i] = n[0]
          continue
        }
        n = Array.from(n)
      } else if (n.constructor === Object) {
        n = Object.values(n)
      }

      if (d.isArr(n)) {
        if (!isnl) {
          n = d.prime.apply(null, n)
          if (n.length < 2) {
            nodes[i] = n[0]
            i--
            continue
          }
        }
        nodes.splice(i, 1, ...n)
        i--
      } else if (n != null) {
        throw new Error(`illegal renderable: ${n}`)
      }
    }
    return nodes
  }

  /*
   * attach renderables to a host node via a connector
   * like append, prepend, before, after
   * independant of load state
   */
  d.attach = (host, connector, ...renderables) => {
    if (host instanceof Function) host = host()
    if (renderables.length === 1 && d.isArr(renderables[0])) renderables = renderables[0]
    const nodeHost = host instanceof Node
    renderables = d.prime(renderables)
    if (nodeHost)
      d.vpend(renderables, host, connector)
    else if (typeof host === 'string')
      return d.queryAsync(host).then(h => d.attach(h, connector, ...renderables))
    if (d.isArr(host)) host.push(...renderables)
    return renderables.length === 1 ? renderables[0] : renderables
  }

  /*
   * render attaches one node to another
   */
  d.render = (node, host = document.body || 'body', connector = 'appendChild') => d.attach(host, connector, node)

  const infinifyDOM = (gen, tag) => tag in gen ? Reflect.get(gen, tag) :
    (gen[tag] = new Proxy(gen.bind(null, tag), {
      get(fn, classes) {
        classes = classes.replace(/_/g, '-').split('.')
        return new Proxy(function () {
          const el = fn.apply(null, arguments)
          el.classList.add(...classes)
          return el
        }, {
          get(_, anotherClass, proxy) {
            classes.push(...anotherClass.replace(/_/g, '-').split('.'))
            return proxy
          }
        })
      }
    }))

  d.actualDF = {}
  const domfn = new Proxy(d, {
    get: (d, key) => d.actualDF[key] || infinifyDOM(d, key),
    set: (d, key, val) => Reflect.set(d.actualDF, key, val)
  })
  

  domfn.css = (node, styles, prop) => {
    if (styles == null) {
      if (document.defaultView) {
        return document.defaultView.getComputedStyle(node)
      }
    } else if (styles.constructor === Object) {
      for (const key in styles) domfn.css(node, key, styles[key])
    } else if (typeof styles === 'string') {
      if (prop == null) {
        if (styles && styles[0] === '-') return node.getPropertyValue(styles)
        if (document.defaultView) {
          const style = document.defaultView.getComputedStyle(node)
          if (style) return styles ? style[styles] : style
        }
      } else if (styles[0] === '-') {
        node.style.setProperty(styles, prop)
      } else {
        node.style[styles] = prop
      }
    }
    return node
  }

  domfn.class = (node, c, state) => {
    if (node && c != null && node.classList) {
      if (d.isArr(node)) {
        for (const n of node) domfn.class(n, c, state)
      } else if (c.constructor === Object) {
        for (const name in c) {
          if (c[name] === true) node.classList.add(name)
          else if (c[name] === false) node.classList.remove(name)
          else for (const cl of c) node.classList.toggle(name)
        }
      } else {
        if (typeof c === 'string') c = c.split(' ')
        for (const cl of c) {
          if (state === true) node.classList.add(cl)
          else if (state === false) node.classList.remove(cl)
          else node.classList.toggle(cl)
        }
      }
    }
    return node
  }

  domfn.hasClass = (node, name) => node.classList.contains(name)

  domfn.attr = (node, attr, val) => {
    if (attr.constructor === Object) {
      for (const a in attr) {
        const present = attr[a] == null
        node[present ? 'removeAttribute' : 'setAttribute'](a, attr[a])
      }
    } else if (typeof attr === 'string') {
      const old = node.getAttribute(attr)
      if (val != null) {
        node.setAttribute(attr, val)
      }
      return old
    }
    return node
  }

  domfn.removeAttribute = (node, ...attrs) => {
    if (attrs.length === 1) node.removeAttribute(attrs[0])
    else for (let i = 0; i < attrs.length; i++) {
      if (d.isArr(attrs[i])) {
        attrs.splice(i, 1, ...attrs[i])
        i--
      }
      node.removeAttribute(attrs[i])
    }
    return node
  }

  domfn.attrToggle = (
    node,
    name,
    state = !node.hasAttribute(name),
    val = node.getAttribute(name) || ''
  ) => {
    if (state) {
      node.setAttribute(name, val)
    } else {
      node.removeAttribute(name)
    }
    return node
  }

  domfn.emit = (node, type, detail) => (node.dispatchEvent(type instanceof String ? new CustomEvent(type, {detail}) : type), node)
  domfn.append = (node, ...children) => (d.attach(node, 'appendChild', ...children), node)
  domfn.prepend = (node, ...children) => (d.attach(node, 'prepend', ...children), node)
  domfn.appendTo = (node, host) => (d.attach(node, 'appendChild', host), node)
  domfn.prependTo = (node, host) => (d.attach(node, 'prepend', host), node),
  domfn.clear = node => (node[d.isInput(node) ? 'value' : 'textContent'] = '', node)

  domfn.refurbish = node => {
    node.removeAttribute('class')
    for (const {name} of node.attributes) node.removeAttribute(name)
    return domfn.clear(node)
  }

  domfn.remove = (node, after) => {
    if (node instanceof Function) node = node()
    if (typeof node === 'string') node = document.querySelector(node)
    if (d.isArr(node))
      for (const n of node) domfn.remove(n, after)
    else if (d.isNum(after)) {
      return new Promise(res => setTimeout(() => res(domfn.remove(node)), after))
    } else if (d.isMounted(node)) d.run(() => node.remove())
    else if (d.isNodeList(node))
      for (let i = 0; i < node.length; i++) domfn.remove(node[i])
    return node
  }

  domfn.replace = (node, newnode) => {
    if (newnode instanceof Function) newnode = newnode()
    d.run(() => node.replaceWith(newnode))
    return newnode
  }

  domfn.find = d.queryAll
  domfn.findOne = d.query
  
  domfn.empty = domfn.clear
  d.domfn = domfn

  d.txt = str => new Text(str)

  d.assimilate = Object.assign((el, {props, methods}) => {
    if (props) d.assimilate.props(el, props)
    if (methods) d.assimilate.methods(el, methods)
  }, {
    props(el, props) {
      for (const prop in props) {
        let val = props[prop]
        if (prop in el) {
          el[prop] = val
        } else if (prop === 'accessors') {
          for (const key in val) {
            const {
              set = val[key], get = val[key]
            } = val[key]
            const accessors = {}
            if (set instanceof Function) {
              accessors.set = set.bind(el, el)
            }
            if (get instanceof Function) {
              accessors.get = get.bind(el, el)
            }
            Object.defineProperty(el, key, accessors)
          }
        } else if (val instanceof Function) {
          el[prop] = val.call(el, el)
        } else {
          copyprop(el, props, prop)
        }
      }
    },
    methods(el, methods) {
      for (const name in methods) Object.defineProperty(el, name, {
        value: methods[name].bind(el, el)
      })
    }
  })

  d.h = (strs, ...args) => {
    let result = ''
    for (let i = 0; i < args.length; i++) result += strs[i] + args[i]
    result += strs[strs.length - 1]

    const template = document.createElement('template')
    template.innerHTML = result
    const {
      content
    } = template

    content.collect = ({
      attr = 'ref',
      keep,
      assign = {}
    } = {}) => {
      Array.from(content.querySelectorAll('[' + attr + ']')).reduce((a, el) => {
        const ref = el.getAttribute(attr).trim()
        if (!keep) el.removeAttribute(attr)
        a[ref] = el
        return a
      }, assign)
      return assign
    }
    content.renderCollect = (...args) => {
      const collection = content.collect()
      d.render(content, ...args)
      return collection
    }
    return content
  }

  const mutateSet = set => (n, state) =>
    set[state == null ? 'has' : state ? 'add' : 'delete'](n)

  const Initiated = new Map()
  const beenInitiated = (name, el) => Initiated.has(name) && Initiated.get(name)(el)

  const attributeObserver = (el, name, opts) => {
    let {init, update, remove} = opts
    if (!init && !update && opts instanceof Function) [init, update] = [opts, opts]
    const intialize = (present, value) => {
      if (present && !beenInitiated(name, el)) {
        if (init) init(el, value)
        if (!Initiated.has(name)) {
          Initiated.set(name, mutateSet(new WeakSet()))
        }
        Initiated.get(name)(el, true)
        return true
      }
      return beenInitiated(name, el)
    }
    let removedBefore = false
    let old = el.getAttribute(name)
    intialize(el.hasAttribute(name), old)
    const stop = d.on.attr(el, ({name: attrName, value, oldvalue, present}) => {
      if (
        attrName === name &&
        old !== value &&
        value !== oldvalue &&
        intialize(present, value)
      ) {
        if (present) {
          if (update) update(el, value, old)
          removedBefore = false
        } else if (!removedBefore) {
          if (remove) remove(el, value, old)
          removedBefore = true
        }
        old = value
      }
    })

    const manager = () => {
      stop()
      if (Initiated.has(name)) Initiated.get(name)(el, false)
    }
    manager.start = () => {
      stop.on()
      Initiated.get(name)(el, true)
    }
    return (manager.stop = manager)
  }

  const directives = new Map()
  d.directive = (name, opts) => {
    const directive = new Map()
    directive.init = el => {
      if (!beenInitiated(name, el))
        directive.set(el, attributeObserver(el, name, opts))
    }
    directive.stop = el => {
      if (directive.has(el)) directive.get(el)()
    }
    directives.set(name, directive)
    d.run(() => d.queryEach('[' + name + ']', n => attributeChange(n, name)))
    return directive
  }

  const attributeChange = (
    el,
    name,
    oldvalue,
    value = el.getAttribute(name),
    present = el.hasAttribute(name)
  ) => {
    if (directives.has(name)) directives.get(name).init(el)
    if (value !== oldvalue) {
      el.dispatchEvent(d.assign(new CustomEvent('attr'), {
        name,
        value,
        oldvalue,
        present
      }))
    }
  }

  d.createElementPlugins = {}

  return d
})(
  function d(tag, ops, ...children) {
    const el = tag.constructor === String ? document.createElement(tag) : tag
    const opsisObj = d.isObj(ops)
    if (opsisObj) {
      d.assimilate(el, ops)
      let val
      for (const key in ops) {
        if ((val = ops[key]) == null) continue

        if (key[0] == 'o' && key[1] == 'n') {
          const isOnce = key[2] == 'c' && key[3] == 'e'
          const i = isOnce ? 4 : 2
          const mode = key.substr(0, i)
          let type = key.substr(i)
          const evtfn = d.EventManager(isOnce)
          if (!ops[mode]) ops[mode] = {}
          if (d.isFunc(val)) {
            ops[mode][type] = evtfn(el, val.name.substr(i), val)
            delete ops[val.name]
          } else {
            const args = d.isArr(val) ? val : [val]
            ops[mode][type] = type.length ? evtfn(el, type, ...args) : evtfn(el, ...args)
          }
        } else if (key in el) {
          if (el[key] instanceof Function) {
            d.isArr(val) ? el[key].apply(el, val) : el[key](val)
          } else {
            el[key] = ops[key]
          }
        } else if (key in d.actualDF) {
          val = d.isArr(val) ? d.actualDF[key](el, ...val) : d.actualDF[key](el, val)
          if (val !== el) ops[key] = val
        } else if (key in d.createElementPlugins) {
          d.createElementPlugins[key](val, el, ops)
        }
      }

      const host = ops.$ || ops.render || ops.$pre
      if (host) {
        d.attach(host, host == ops.$pre ? 'prepend' : 'appendChild', el)
      }
    }

    if (el.nodeType !== 3) {
      if (!opsisObj) {
        if (ops instanceof Function) {
          const result = ops.call(el, el)
          ops = result !== el ? result : undefined
        }
        if (d.isRenderable(ops)) children.unshift(ops)
      }
      if (children.length) d.attach(el, 'appendChild', children)
    }
    return el
  }
)