const version = 'v0.0.4'
const cacheName = version + '-kurshok.space'

const cachedPaths = [
  '/css/marx.min.css',
  '/css/site.min.css',
  '/css/fontello/css/fontello.min.css',
  '/css/fontello/font/fontello.woff2?9565335',
  '/css/auth.min.css',
  '/css/postauth.min.css',
  '/css/SuperTinyIcons/amazon.svg',
  '/css/SuperTinyIcons/paypal.svg',
  '/css/SuperTinyIcons/digitalocean.svg',
  '/css/SuperTinyIcons/opensource.svg',
  '/css/SuperTinyIcons/ethereum.svg',
  '/css/SuperTinyIcons/github.svg',
  '/js/domlib.min.js',
  '/js/router.min.js',
  '/js/site.min.js',
  '/js/writ-writer.min.js',
  '/js/localforage.min.js',
  '/js/profile.min.js',
  /* 
  '/',
  '/css/style.css',
  '/js/admin.js',
  '/js/main.js',
  '/js/auth.js',
  '/js/view.js', */
]

self.addEventListener('install', e => {
  e.waitUntil(caches.open(cacheName)
    .then(cache => cache.addAll(cachedPaths))
  )
})

const unableToResolve = () => new Response('<h1>Service Unavailable</h1>', {
  status: 503,
  statusText: 'Service Unavailable',
  headers: new Headers({
    'Content-Type': 'text/html'
  })
})

self.addEventListener('fetch', e => {
  if (cachedPaths.some(path => e.request.url.includes(path))) {
    e.respondWith(caches.match(e.request)
      .then(res => res || fetch(e.request))
    )
  }
})

self.addEventListener("activate", event => {
  event.waitUntil(
    caches.keys()
    .then(keys => Promise.all(keys
      .filter(key => !key.startsWith(version))
      .map(key => caches.delete(key))
    ))
  )
})