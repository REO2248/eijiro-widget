pkgname=eijiro-widget
pkgver=0.1.0
pkgrel=1
pkgdesc="Modern GTK dictionary widget for Eijiro"
arch=('x86_64')
license=('MIT')
depends=('libadwaita' 'gtk4' 'gtk4-layer-shell')
makedepends=('rust' 'cargo')
source=()
sha256sums=()

prepare() {
  cd "$startdir"
  cargo fetch --locked
}

build() {
  cd "$startdir"
  cargo build --release --frozen
}

package() {
  cd "$startdir"
  install -Dm755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
  
  install -Dm644 "$pkgname.desktop" "$pkgdir/usr/share/applications/$pkgname.desktop"
  
  install -Dm644 "LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
  
  install -Dm644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
}
