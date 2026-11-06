const tg = window.Telegram?.WebApp;
tg?.ready();
tg?.expand();

const user = tg?.initDataUnsafe?.user;
const userId = user ? user.id : 0;
const userName = user ? user.first_name : 'Гость';
document.getElementById('greeting').textContent = `Привет, ${userName}!`;

let categories = [];
let products = [];
let activeCategoryId = null;
let cart = [];
let selectedProduct = null;
let statusInterval = null;
let orderStartTime = null;

async function init() {
    try {
        const res = await fetch('/api/menu');
        const data = await res.json();
        categories = data.categories;
        products = data.products;
        
        renderSidebar();
        if (categories.length > 0) {
            selectCategory(categories[0].id);
        }
        
        document.getElementById('loader').style.display = 'none';
        checkActiveOrder();
    } catch (e) {
        alert('Ошибка загрузки меню');
    }
}

function renderSidebar() {
    const sidebar = document.getElementById('sidebar');
    sidebar.innerHTML = categories.map(c => `
        <button onclick="selectCategory(${c.id})" id="cat-${c.id}">${c.name}</button>
    `).join('');
}

function selectCategory(id) {
    activeCategoryId = id;
    document.querySelectorAll('.sidebar button').forEach(b => b.classList.remove('active'));
    document.getElementById(`cat-${id}`).classList.add('active');
    renderProducts();
}

function renderProducts() {
    const area = document.getElementById('products-area');
    const filtered = products.filter(p => p.category_id === activeCategoryId && !p.is_archived);
    
    if (filtered.length === 0) {
        area.innerHTML = '<p style="color:#94a3b8;text-align:center;grid-column:span 2;margin-top:20px;">В этой категории пока пусто</p>';
        return;
    }

    area.innerHTML = filtered.map(p => `
        <div class="product-card" onclick="openProduct(${p.id})">
            ${p.image_url ? `<img src="${p.image_url}" class="product-img">` : `<div class="product-img" style="display:flex;align-items:center;justify-content:center;font-size:30px;">🍽</div>`}
            <div class="product-title">${p.name}</div>
            <div class="product-desc">${p.description || ''}</div>
            <div class="product-bottom">
                <div class="product-price">${p.variants[0].price} GEL</div>
                <button class="btn-add">+</button>
            </div>
        </div>
    `).join('');
}

function openProduct(id) {
    selectedProduct = products.find(p => p.id === id);
    if (!selectedProduct) return;

    document.getElementById('size-modal-title').textContent = selectedProduct.name;
    const options = document.getElementById('size-options');
    
    if (selectedProduct.variants.length === 1) {
        addToCart(selectedProduct, selectedProduct.variants[0]);
        return;
    }

    options.innerHTML = selectedProduct.variants.map(v => `
        <div class="variant-option" onclick='addToCart(selectedProduct, ${JSON.stringify(v)})'>
            <span>${v.name}</span>
            <span>${v.price} GEL</span>
        </div>
    `).join('');
    
    openModal('size-modal');
}

function addToCart(product, variant) {
    const existing = cart.find(i => i.variant.id === variant.id);
    if (existing) {
        existing.qty += 1;
    } else {
        cart.push({ product, variant, qty: 1 });
    }
    updateCartUI();
    closeModal('size-modal');
    if (tg) tg.HapticFeedback.impactOccurred('light');
}

function updateCartUI() {
    let total = 0;
    let count = 0;
    cart.forEach(i => { total += i.variant.price * i.qty; count += i.qty; });
    
    const fab = document.getElementById('cart-fab');
    document.getElementById('cart-total').textContent = `${total} GEL`;
    
    if (count > 0) fab.classList.add('visible');
    else fab.classList.remove('visible');

    const cartItems = document.getElementById('cart-items');
    cartItems.innerHTML = cart.map((i, index) => `
        <div class="cart-item">
            <div class="cart-item-info">
                <h4>${i.product.name}</h4>
                <p>${i.variant.name} • ${i.variant.price} GEL</p>
            </div>
            <div class="cart-controls">
                <button onclick="changeQty(${index}, -1)">-</button>
                <span>${i.qty}</span>
                <button onclick="changeQty(${index}, 1)">+</button>
            </div>
        </div>
    `).join('');
}

function changeQty(index, delta) {
    cart[index].qty += delta;
    if (cart[index].qty <= 0) cart.splice(index, 1);
    updateCartUI();
}

async function submitOrder() {
    if (cart.length === 0) return;
    
    const comment = document.getElementById('order-comment').value;
    const items = cart.map(i => ({ variant_id: i.variant.id, qty: i.qty }));

    const res = await fetch('/api/order', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ user_id: userId, user_name: userName, items, comment })
    });

    const data = await res.json();
    
    if (data.ok) {
        cart = [];
        updateCartUI();
        closeModal('cart-modal');
        document.getElementById('order-comment').value = '';
        
        if (tg) tg.HapticFeedback.notificationOccurred('success');
        showStatus(data.order_number);
    } else {
        alert(data.error || 'Ошибка при отправке');
    }
}

function showStatus(orderNum) {
    document.getElementById('status-screen').style.display = 'flex';
    document.getElementById('status-order-num').textContent = `№${orderNum}`;
    orderStartTime = Date.now();
    
    document.getElementById('step-new').className = 'step done';
    document.getElementById('step-accepted').className = 'step active';
    document.getElementById('step-ready').className = 'step';
    document.getElementById('status-title').textContent = 'Ожидает принятия';
    document.getElementById('btn-new-order').style.display = 'none';

    if (statusInterval) clearInterval(statusInterval);
    statusInterval = setInterval(checkOrderStatus, 2000);
}

async function checkOrderStatus() {
    const res = await fetch(`/api/orders`);
    const orders = await res.json();
    const myOrder = orders.find(o => o.user_id === userId);
    
    if (myOrder) {
        if (myOrder.status === 'accepted') {
            document.getElementById('status-title').textContent = 'Готовится';
            document.getElementById('step-accepted').className = 'step done';
            document.getElementById('step-ready').className = 'step active';
        } else if (myOrder.status === 'ready') {
            document.getElementById('status-title').textContent = 'Готов к выдаче!';
            document.getElementById('step-ready').className = 'step done';
            document.getElementById('btn-new-order').style.display = 'block';
            clearInterval(statusInterval);
            if (tg) tg.HapticFeedback.notificationOccurred('success');
        }
    }
    
    const elapsed = Math.floor((Date.now() - orderStartTime) / 1000);
    const m = String(Math.floor(elapsed / 60)).padStart(2, '0');
    const s = String(elapsed % 60).padStart(2, '0');
    document.getElementById('status-timer').textContent = `${m}:${s}`;
}

async function checkActiveOrder() {
    if (!userId) return;
    const res = await fetch(`/api/orders`);
    const orders = await res.json();
    const myOrder = orders.find(o => o.user_id === userId);
    if (myOrder) {
        showStatus(myOrder.order_number);
    }
}

function resetToMenu() {
    document.getElementById('status-screen').style.display = 'none';
}
function openCart() { openModal('cart-modal'); }
function openModal(id) { document.getElementById(id).classList.add('open'); }
function closeModal(id) { document.getElementById(id).classList.remove('open'); }

init();
