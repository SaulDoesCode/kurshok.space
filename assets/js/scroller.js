import d from '/js/domlib.min.js'

/**
 * Uses Intersection Observer to monitor the page location of a series of
 * elements for scrollytelling.
 *
 * @param {object} options
 * @param {Element} [options.container] Optionally pass in what should be
 * considered the containing element of all the scenes - this gets added to the
 * Intersection Observer instance and additionally fires its own events
 * @param {Number} [options.offset] How far from the top/bottom of the viewable
 * area to trigger enters/exits of scenes, represented as a value between
 * 0 and 1
 * @param {Element[]} options.scenes An array of all the Elements to be
 * considered scenes of this Scroller
 * @property {IntersectionObserver|null} observer Once initialized, a reference
 * to the Scroller's instance of IntersectionObserver
 * @example
 *
 * import Scroller from '@newswire/scroller';
 *
 * const scroller = new Scroller({
 *   scenes: document.querySelectorAll('.scenes')
 * });
 *
 * scroller.init()
 */

const Scroller = ({container, offset = 0.25, scenes = [], init = true}) => {
    let previousOffset = 0, observer

    if (typeof scenes === 'string') {
        scenes = d.queryAll(scenes, container)
        if (scenes == null || scenes.length == 0) throw new Error('no valid scenes to observe')
    }
    scenes = new Set([...scenes])
    scenes.forEach((scene) => {
        if (typeof scene === 'string') {
            scenes.delete(scene)
            scene = d.queryAll(scene, container)
            if (scene == null) return
            if (scene.length) scene.forEach(s => {
                scenes.add(s)
            })
        }
    })

    /**
     * Determines whether the page was scrolling up or down when an intersection
     * event is triggered. Keeps track of direction via storage of the previous
     * pageYOffset.
     *
     * @private
     * @returns {boolean} If true, the page was scrolling down
     */
    const getDirection = () => {
        const currentOffset = window.pageYOffset
        const isScrollingDown = currentOffset > previousOffset
        previousOffset = currentOffset
        return isScrollingDown
    }


    const s = d.emitter({
        /**
         * Initializes a Scroller's IntersectionObserver on a page and begins sending
         * any intersection events that occur.
         *
         * @returns {void}
         * @example
         *
         * const scroller = new Scroller({
         *   scenes: document.querySelectorAll('.scenes')
         * });
         *
         * scroller.init();
         */
        init() {
            const observed = []

            observer = new IntersectionObserver(entries => {
                const isScrollingDown = getDirection()

                entries.forEach(entry => {
                    const element = entry.target

                    const payload = {
                        bounds: entry.boundingClientRect,
                        element,
                        index: observed.indexOf(element),
                        isScrollingDown,
                    }

                    const prefix = element === container ? 'container' : 'scene'

                    if (entry.isIntersecting) {
                        /**
                         * Container enter event. Fires whenever the container begins intersecting.
                         *
                         * @event Scroller#container:enter
                         * @type {object}
                         * @property {DOMRectReadOnly} bounds The bounds of the active element
                         * @property {Element} element The element that intersected
                         * @property {number} index This is always -1 on the container
                         * @property {boolean} isScrollingDown Whether the user triggered this element
                         * while scrolling down or not
                         */
                        /**
                         * Scene enter event. Fires whenever a scene begins intersecting.
                         *
                         * @event Scroller#scene:enter
                         * @type {object}
                         * @property {DOMRectReadOnly} bounds The bounds of the active element
                         * @property {Element} element The element that intersected
                         * @property {number} index The index of the active element
                         * @property {boolean} isScrollingDown Whether the user triggered this element
                         * while scrolling down or not
                         */
                        s.emit(`${prefix}:enter`, payload)
                    } else {
                        /**
                         * Container exit event. Fires whenever the container has exited.
                         *
                         * @event Scroller#container:exit
                         * @type {object}
                         * @property {DOMRectReadOnly} bounds The bounds of the exiting element
                         * @property {Element} element The element that exited
                         * @property {number} index This is always -1 on the container
                         * @property {boolean} isScrollingDown Whether the user triggering the exit
                         * while scrolling down or not
                         */
                        /**
                         * Scene enter event. Fires whenever a scene has exited.
                         *
                         * @event Scroller#scene:exit
                         * @type {object}
                         * @property {DOMRectReadOnly} bounds The bounds of the exiting element
                         * @property {Element} element The element that exited
                         * @property {number} index The index of the exiting element
                         * @property {boolean} isScrollingDown Whether the user triggering the exit
                         * while scrolling down or not
                         */
                        s.emit(`${prefix}:exit`, payload)
                    }
                })
            }, {
                    rootMargin: `${-100 * (1 - offset)}% 0px ${-100 * offset}%`,
            })

            scenes.forEach(item => {
                observed.push(item)
                observer.observe(item)
            })

            // a container is not required, but if provided we'll track it
            if (container) observer.observe(container)

            /**
             * Init event. Fires once Scroller has finished setting up.
             *
             * @event Scroller#init
             */
            s.emit('init')
        }
    })

    if (init) s.init()

    return s
}

export default Scroller