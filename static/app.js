//Global variables
let currentUser = {
    address: '',
    name: ''
};

let walletProvider = null;
let walletAccount = null;

let currentPostId = null;
let currentPostImage = null;
let currentCommentImage = null;

//Page related variables
let currentOffset = 0;
const POSTS_PER_PAGE = 20;
let isLoadingMore = false;
let hasMorePosts = true;

//Prevent duplicate submission of related variables
let isSubmittingPost = false;
let isSubmittingComment = false;

// In production, mute debug logs by default (enable via window.IRYS_DEBUG = true)
if (typeof window !== 'undefined') {
    window.IRYS_DEBUG = window.IRYS_DEBUG ?? false;
    if (!window.IRYS_DEBUG && typeof console !== 'undefined') {
        ['log', 'info', 'debug', 'warn'].forEach(function(m){ if (typeof console[m] === 'function') { console[m] = function(){}; } });
    }
}


// Request queue to limit concurrent requests and avoid conflicts
class RequestQueue {
    constructor(maxConcurrent = 3) {
        this.queue = [];
        this.running = 0;
        this.maxConcurrent = maxConcurrent;
    }
    
    async add(requestFn) {
        return new Promise((resolve, reject) => {
            this.queue.push({
                fn: requestFn,
                resolve,
                reject
            });
            this.process();
        });
    }
    
    async process() {
        if (this.running >= this.maxConcurrent || this.queue.length === 0) {
            return;
        }
        
        this.running++;
        const { fn, resolve, reject } = this.queue.shift();
        
        try {
            const result = await fn();
            resolve(result);
        } catch (error) {
            reject(error);
        } finally {
            this.running--;
            this.process(); // Process next request
        }
    }
}

// Global request queue instance
const requestQueue = new RequestQueue(3);

// Performance monitor
class PerformanceMonitor {
    constructor() {
        this.metrics = {
            totalRequests: 0,
            completedRequests: 0,
            failedRequests: 0,
            avgResponseTime: 0,
            responseTimes: []
        };
    }
    
    startRequest() {
        this.metrics.totalRequests++;
        return Date.now();
    }
    
    endRequest(startTime, success = true) {
        const duration = Date.now() - startTime;
        this.metrics.responseTimes.push(duration);
        this.metrics.completedRequests++;
        
        if (!success) {
            this.metrics.failedRequests++;
        }
        
        // Calculate average response time (keep the last 100 samples)
        if (this.metrics.responseTimes.length > 100) {
            this.metrics.responseTimes = this.metrics.responseTimes.slice(-100);
        }
        
        this.metrics.avgResponseTime = this.metrics.responseTimes.reduce((a, b) => a + b, 0) / this.metrics.responseTimes.length;
        
        this.updateDisplay();
    }
    
    updateDisplay() {
        const queueSize = requestQueue.queue.length;
        const running = requestQueue.running;
        
        console.log(`📊 Performance stats:`, {
            queueSize: queueSize,
            running: running,
            totalRequests: this.metrics.totalRequests,
            completedRequests: this.metrics.completedRequests,
            failedRequests: this.metrics.failedRequests,
            avgResponseTime: Math.round(this.metrics.avgResponseTime) + 'ms'
        });
    }
    
    getStats() {
        return {
            ...this.metrics,
            queueSize: requestQueue.queue.length,
            activeRequests: requestQueue.running
        };
    }
}

//Global Performance Monitor
const performanceMonitor = new PerformanceMonitor();


async function retryRequest(requestFn, maxRetries = 3, delay = 1000) {
    for (let i = 0; i < maxRetries; i++) {
        try {
            return await requestFn();
        } catch (error) {
            console.log(`⚠️ Request failed (${i + 1}/${maxRetries}):`, error.message);
            
            if (i === maxRetries - 1) {
                throw error; 
            }
            

            await new Promise(resolve => setTimeout(resolve, delay * Math.pow(2, i)));
        }
    }
}


const API_BASE = '/api';


const CONTRACT_ADDRESS = '0xBebfAC28e35c7a70eAe9Df606199E45c85a73a9a';
const CONTRACT_ABI = [
    "function createPost(string memory _title, string memory _content, string[] memory _tags, string memory _irysTransactionId) external payable",
    "function createComment(uint256 _postId, string memory _content, uint256 _parentId, string memory _irysTransactionId) external payable",
    "function likePost(uint256 _postId) external",
    "function likeComment(uint256 _commentId) external",
    "function registerUsername(string memory _username) external payable",
    "function isUsernameAvailable(string memory _username) external view returns (bool)",
    "function getUsernameByAddress(address _user) external view returns (string memory)",
    "function getAddressByUsername(string memory _username) external view returns (address)",
    "function postCost() external view returns (uint256)",
    "function commentCost() external view returns (uint256)",
    "function usernameCost() external view returns (uint256)",
    "event PostCreated(uint256 indexed postId, address indexed author, string title, uint256 pointsEarned)",
    "event CommentCreated(uint256 indexed commentId, uint256 indexed postId, address indexed author, uint256 pointsEarned)",
    "event UsernameRegistered(address indexed user, string username)",
    "event PointsEarned(address indexed user, uint256 points, string reason)"
];

// Initialize app
document.addEventListener('DOMContentLoaded', async function() {

    

    const networkStatus = document.getElementById('networkStatus');
    if (networkStatus) {

        networkStatus.className = 'status-dot offline';

    } else {
        /* ignore: missing networkStatus element on DOMContentLoaded */
    }
    
    await initializeApp();
    loadPosts();
    setupEventListeners();
    updateNetworkStatus();
    

    initializeImagePreviews();
    

    loadGlobalStats();
    loadActiveUsers();
    

    switchView('posts');
});


function loadUserProfile() {
    const profileSection = document.getElementById('profile');
    if (!profileSection) return;
    
    if (!walletAccount) {
        profileSection.innerHTML = `
            <div class="wallet-connect-prompt">
                <div class="wallet-prompt-card">
                    <div class="wallet-icon">👤</div>
                    <h3>Connect wallet to view personal information</h3>
                    <p>Please connect your wallet first to view and manage your personal information</p>
                    <button class="connect-wallet-btn" onclick="connectWallet()">
                        <span class="wallet-btn-icon">🦊</span>
                        Connect MetaMask wallet
                    </button>
                    <div class="wallet-tips">
                        <p>💡 After connecting the wallet, you can:</p>
                        <ul>
                            <li>View all the posts you have posted</li>
                            <li>Manage post content</li>
                            <li>View post data statistics</li>
                        </ul>
                    </div>
                </div>
            </div>
        `;
        return;
    }
    
    profileSection.innerHTML = `
        <div class="user-profile">
            <h3>My profile</h3>
            <div class="profile-info">
                <div class="profile-item">
                    <span class="label">Wallet address:</span>
                    <span class="value">${walletAccount.slice(0, 6)}...${walletAccount.slice(-4)}</span>
                </div>
                <div class="profile-item">
                    <span class="label">balance:</span>
                    <span class="value" id="profileBalance">LOADING...</span>
                </div>
                <div class="profile-item">
                    <span class="label">NAME:</span>
                    <span class="value">${currentUser.username || 'No setting'}</span>
                </div>
            </div>
        </div>
    `;
    
    // Update balance display
    updateBalance().then(() => {
        const profileBalanceEl = document.getElementById('profileBalance');
        if (profileBalanceEl && currentUser.balance) {
            profileBalanceEl.textContent = `${currentUser.balance} IRYS`;
        }
    });
}

// My Posts pagination state
let myPostsOffset = 0;
const MY_POSTS_PER_PAGE = 20;
let isLoadingMoreMyPosts = false;
let hasMoreMyPosts = true;
let allMyPosts = [];

// Load My Posts (paginated)
async function loadMyPosts(append = false) {
    const myPostsSection = document.getElementById('my-posts');
    if (!myPostsSection) return;
    
    // Check wallet connection state
    if (!walletAccount || !currentUser.address) {
    myPostsSection.innerHTML = `
            <div class="wallet-connect-prompt">
                <div class="wallet-prompt-card">
                    <div class="wallet-icon">
                        <img src="/icon/Eyes_Closed_Sprite.webp" alt="My posts">
                    </div>
                    <h3>Connect wallet to view my posts</h3>
                    <p>Please connect your wallet first to view and manage the posts you have posted</p>
                    <button class="connect-wallet-btn" onclick="connectWallet()">
                        <span class="wallet-btn-icon">🦊</span>
                        connect MetaMask wallet
                    </button>
                    <div class="wallet-tips">
                        <p>💡 After connecting the wallet, you can：</p>
                        <ul>
                            <li>View all the posts you have posted</li>
                            <li>Manage post content</li>
                            <li>View post data statistics</li>
                        </ul>
                    </div>
                </div>
            </div>
        `;
        return;
    }
    
    if (isLoadingMoreMyPosts) return;
    
    try {
        if (!append) {
         
            myPostsOffset = 0;
            hasMoreMyPosts = true;
            allMyPosts = [];
    
   
    myPostsSection.innerHTML = `
        <div class="my-posts-container">
            <div class="my-posts-header">
            <h3>My posts</h3>
                <div class="user-info">
                    <span class="user-address">Address: ${currentUser.address.substring(0, 6)}...${currentUser.address.substring(38)}</span>
                </div>
            </div>
            <div class="loading-container">
                <div class="loading-spinner"></div>
                <p>Loading...</p>
            </div>
        </div>
    `;
        }
    
        isLoadingMoreMyPosts = true;
        
   
        const response = await fetch(`/api/users/${currentUser.address}/posts?limit=${MY_POSTS_PER_PAGE}&offset=${myPostsOffset}&user_address=${encodeURIComponent(walletAccount)}`);
        const result = await response.json();
        
        if (result.success && Array.isArray(result.data)) {
            const newPosts = result.data;
            
            if (newPosts.length < MY_POSTS_PER_PAGE) {
                hasMoreMyPosts = false;
            }
            
            if (append) {
                allMyPosts = allMyPosts.concat(newPosts);
            } else {
                allMyPosts = newPosts;
                
                if (allMyPosts.length === 0) {
                myPostsSection.innerHTML = `
                    <div class="my-posts-container">
                        <div class="my-posts-header">
                            <h3>My posts</h3>
                            <div class="user-info">
                                <span class="user-address">address: ${currentUser.address.substring(0, 6)}...${currentUser.address.substring(38)}</span>
                            </div>
                        </div>
                        <div class="empty-posts">
                            <div class="empty-icon">📝</div>
                            <h4>No posts</h4>
                            <p>Click on the 'My Posts' tab to start posting your first post!</p>
                            <button class="create-post-btn" onclick="switchView('posts')">
                                Post now
                            </button>
                        </div>
                    </div>
                `;
                return;
                }
            }
            
            myPostsOffset += newPosts.length;
            
         
            displayMyPosts(allMyPosts, !append);
            
        } else {
            throw new Error(result.error || 'Failed to retrieve post');
        }
        
    } catch (error) {
        console.error('Failed to load My Posts:', error);
        if (!append) {
            myPostsSection.innerHTML = `
                <div class="my-posts-container">
                    <div class="my-posts-header">
                        <h3>My posts</h3>
                    </div>
                    <div class="error-message">
                        <div class="error-icon">❌</div>
                        <h4>Loading failed</h4>
                        <p>Unable to load your post: ${error. message}</p>
                        <button class="retry btn" onclick="loadMyPosts()">retry</button>
                    </div>
                </div>
            `;
        }
    } finally {
        isLoadingMoreMyPosts = false;
    }
}


function displayMyPosts(posts, showHeader = true) {
    const myPostsSection = document.getElementById('my-posts');
    if (!myPostsSection) return;
    
            const postsHtml = posts.map(post => `
                <div class="post-card my-post-card" data-post-id="${post.id}">
                    <div class="post-header">
                        <div class="post-meta">
                            <span class="post-date">${formatTime(post.created_at)}</span>
                            <div class="post-stats">
                        <span class="stat-item like-btn-card ${post.is_liked_by_user ? 'liked' : ''}" onclick="event.stopPropagation(); likePost('${post.id}', this)" data-post-id="${post.id}">
                            ${post.is_liked_by_user ? '<img src="/icon/Group_1073717789.webp" alt="Already liked" class="like-icon">' : '<i class="far fa-heart"></i>'}
                            <span>${post.likes || 0}</span>
                        </span>
                        <span class="stat-item">
                            <i class="fas fa-comment"></i>
                            <span>${post.comments_count || 0}</span>
                        </span>
                            </div>
                        </div>
                    </div>
                    <div class="post-content">
                        <h4 class="post-title">${escapeHtml(post.title)}</h4>
                        <p class="post-preview">${escapeHtml(post.content.substring(0, 150))}${post.content.length > 150 ? '...' : ''}</p>
                        ${post.image ? `<div class="post-image-preview"><img src="${post.image}" alt="Post image" onclick="showImageModal('${post.image}')"></div>` : ''}
                    </div>
                    <div class="post-actions">
                        <button class="view-post-btn" onclick="openPost('${post.id}')">
                            View details
                        </button>
                        <div class="post-tags">
                            ${post.tags && post.tags.length > 0 ? post.tags.map(tag => `<span class="tag">${escapeHtml(tag)}</span>`).join('') : ''}
                        </div>
                    </div>
                </div>
            `).join('');
            
    // "Load More"
    const loadMoreButton = hasMoreMyPosts ? `
        <div class="load-more-container">
            <button class="load-more-btn" onclick="loadMoreMyPosts()" ${isLoadingMoreMyPosts ? 'disabled' : ''}>
                ${isLoadingMoreMyPosts ? '<i class="fas fa-spinner fa-spin"></i> LOADING...' : '<i class="fas fa-chevron-down"></i> Loading more posts'}
            </button>
        </div>
    ` : '';
    
    const content = showHeader ? `
                <div class="my-posts-container">
                    <div class="my-posts-header">
                        <h3>My posts</h3>
                        <div class="my-posts-stats">
                    <span class="posts-count">displayed ${posts.length} A post</span>
                            <div class="user-info">
                                <span class="user-address">Address: ${currentUser.address.substring(0, 6)}...${currentUser.address.substring(38)}</span>
                            </div>
                        </div>
                    </div>
                    <div class="my-posts-list">
                        ${postsHtml}
                    </div>
            ${loadMoreButton}
                </div>
    ` : `
        <div class="my-posts-list">
            ${postsHtml}
        </div>
        ${loadMoreButton}
            `;
            
    if (showHeader) {
        myPostsSection.innerHTML = content;
        } else {
      
        const container = myPostsSection.querySelector('.my-posts-container');
        const existingList = container.querySelector('.my-posts-list');
        const existingButton = container.querySelector('.load-more-container');
        
        if (existingList) {
            existingList.innerHTML = postsHtml;
        }
        if (existingButton) {
            existingButton.remove();
        }
        if (loadMoreButton) {
            container.insertAdjacentHTML('beforeend', loadMoreButton);
        }
        
        
        const postsCount = container.querySelector('.posts-count');
        if (postsCount) {
            postsCount.textContent = `displayed ${posts.length} a posts`;
        }
    }
}


async function loadMoreMyPosts() {
    if (!hasMoreMyPosts || isLoadingMoreMyPosts) return;
    await loadMyPosts(true);
}


function loadIrysContent() {
    const irysSection = document.getElementById('irys');
    if (!irysSection) return;
    

    if (!currentUser.address) {
    irysSection.innerHTML = `
            <div class="wallet-connect-prompt">
                <div class="wallet-prompt-card">
                    <div class="wallet-icon">
                        <img src="/icon/Magnifying_Sprite.webp" alt="Follows">
                </div>
                    <h3>Connect wallet to view and follow</h3>
                    <p>Please connect your wallet first to view and manage your following relationships</p>
                    <button class="connect-wallet-btn" onclick="connectWallet()">
                        <span class="wallet-btn-icon">🦊</span>
                        Connect MetaMask wallet
                    </button>
                    <div class="wallet-tips">
                        <p>💡 After connecting the wallet, you can：</p>
                        <ul>
                            <li>View all users you follow</li>
                            <li>Manage Focus Relationships</li>
                            <li>View and follow data statistics</li>
                        </ul>
                    </div>
                </div>
        </div>
    `;
        return;
    }
    
    irysSection.innerHTML = `
        <div class="follow-system">
            <div class="follow-header">
                <h3><i class="fas fa-users"></i> Follows</h3>
                <div class="follow-stats" id="followStats">
                    <div class="stat-item">
                        <div class="stat-number" id="followingCount">-</div>
                        <div class="stat-label">follow</div>
                    </div>
                    <div class="stat-item">
                        <div class="stat-number" id="followersCount">-</div>
                        <div class="stat-label">fans</div>
                    </div>
                    <div class="stat-item">
                        <div class="stat-number" id="friendsCount">-</div>
                        <div class="stat-label">friend</div>
                    </div>
                </div>
            </div>
            
            <div class="follow-tabs">
                <button class="tab-btn active" onclick="switchFollowTab('following')">
                    <i class="fas fa-heart"></i> follow (<span id="followingTabCount">0</span>)
                </button>
                <button class="tab-btn" onclick="switchFollowTab('followers')">
                    <i class="fas fa-users"></i> fans (<span id="followersTabCount">0</span>)
                </button>
                <button class="tab-btn" onclick="switchFollowTab('friends')">
                    <i class="fas fa-user-friends"></i> friend (<span id="friendsTabCount">0</span>)
                </button>
            </div>
            
            <div class="follow-content">
                <div class="tab-content active" id="followingTab">
                    <div class="user-search">
                        <div class="search-box">
                            <input type="text" id="followSearch" placeholder="Search for user address or username..." />
                            <button class="search-btn" onclick="searchUsers()">
                                <i class="fas fa-search"></i>
                            </button>
                        </div>
                    </div>
                    <div class="users-list" id="followingList">
                        <div class="loading-state">
                            <i class="fas fa-spinner fa-spin"></i> loading...
                        </div>
                    </div>
                </div>
                
                <div class="tab-content" id="followersTab">
                    <div class="users-list" id="followersList">
                        <div class="loading-state">
                            <i class="fas fa-spinner fa-spin"></i> loading...
                        </div>
                    </div>
                </div>
                
                <div class="tab-content" id="friendsTab">
                    <div class="users-list" id="friendsList">
                        <div class="loading-state">
                            <i class="fas fa-spinner fa-spin"></i> loading...
                        </div>
                    </div>
                </div>
            </div>
        </div>
    `;
    
  
    loadFollowStats();
    loadFollowingList();
}


let followingOffset = 0;
let followersOffset = 0;
let friendsOffset = 0;
const FOLLOW_USERS_PER_PAGE = 20;
let isLoadingMoreFollowing = false;
let isLoadingMoreFollowers = false;
let isLoadingMoreFriends = false;
let hasMoreFollowing = true;
let hasMoreFollowers = true;
let hasMoreFriends = true;
let allFollowing = [];
let allFollowers = [];
let allFriends = [];


function switchFollowTab(tabName) {
  
    document.querySelectorAll('.tab-btn').forEach(btn => btn.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(content => content.classList.remove('active'));
    
  
    event.target.classList.add('active');
    document.getElementById(tabName + 'Tab').classList.add('active');
    

    switch(tabName) {
        case 'following':
            loadFollowingList();
            break;
        case 'followers':
            loadFollowersList();
            break;
        case 'friends':
            loadFriendsList();
            break;
    }
}


async function loadFollowStats() {
    if (!currentUser.address) return;
    
    try {
        const response = await fetch(`${API_BASE}/users/${currentUser.address}/follow-stats`);
        const result = await response.json();
        
        if (result.success && result.data) {
            const { following_count, followers_count, mutual_follows_count } = result.data;
            
       
            document.getElementById('followingCount').textContent = following_count;
            document.getElementById('followersCount').textContent = followers_count;
            document.getElementById('friendsCount').textContent = mutual_follows_count;
            
       
            document.getElementById('followingTabCount').textContent = following_count;
            document.getElementById('followersTabCount').textContent = followers_count;
            document.getElementById('friendsTabCount').textContent = mutual_follows_count;
        }
    } catch (error) {
        console.error('Failed to load follow statistics:', error);
    }
}


async function loadFollowingList(append = false) {
    if (!walletAccount || isLoadingMoreFollowing) return;
    
    const container = document.getElementById('followingList');
    
    try {
        if (!append) {
            followingOffset = 0;
            hasMoreFollowing = true;
            allFollowing = [];
    container.innerHTML = '<div class="loading-state"><i class="fas fa-spinner fa-spin"></i> loading...</div>';
        }
        
        isLoadingMoreFollowing = true;
        
        const response = await fetch(`${API_BASE}/users/${walletAccount}/following?limit=${FOLLOW_USERS_PER_PAGE}&offset=${followingOffset}`);
        const result = await response.json();
        
        if (result.success && result.data) {
            const newUsers = result.data;
            
            if (newUsers.length < FOLLOW_USERS_PER_PAGE) {
                hasMoreFollowing = false;
            }
            
            if (append) {
                allFollowing = allFollowing.concat(newUsers);
        } else {
                allFollowing = newUsers;
            }
            
            followingOffset += newUsers.length;
            displayUsersList(allFollowing, container, 'following', hasMoreFollowing);
        } else {
            if (!append) {
            container.innerHTML = '<div class="empty-state">No users currently following</div>';
            }
        }
    } catch (error) {
        console.error('Failed to load following list:', error);
        if (!append) {
        container.innerHTML = '<div class="error-state">Loading failed, please try again later</div>';
    }
    } finally {
        isLoadingMoreFollowing = false;
    }
}


async function loadFollowersList(append = false) {
    if (!walletAccount || isLoadingMoreFollowers) return;
    
    const container = document.getElementById('followersList');
    
    try {
        if (!append) {
            followersOffset = 0;
            hasMoreFollowers = true;
            allFollowers = [];
    container.innerHTML = '<div class="loading-state"><i class="fas fa-spinner fa-spin"></i> loading...</div>';
        }
        
        isLoadingMoreFollowers = true;
        
        const response = await fetch(`${API_BASE}/users/${walletAccount}/followers?limit=${FOLLOW_USERS_PER_PAGE}&offset=${followersOffset}`);
        const result = await response.json();
        
        if (result.success && result.data) {
            const newUsers = result.data;
            
            if (newUsers.length < FOLLOW_USERS_PER_PAGE) {
                hasMoreFollowers = false;
            }
            
            if (append) {
                allFollowers = allFollowers.concat(newUsers);
        } else {
                allFollowers = newUsers;
            }
            
            followersOffset += newUsers.length;
            displayUsersList(allFollowers, container, 'followers', hasMoreFollowers);
        } else {
            if (!append) {
            container.innerHTML = '<div class="empty-state">No fans at the moment</div>';
            }
        }
    } catch (error) {
        console.error('Failed to load followers list:', error);
        if (!append) {
        container.innerHTML = '<div class="error-state">Loading failed, please try again later</div>';
    }
    } finally {
        isLoadingMoreFollowers = false;
    }
}


async function loadFriendsList(append = false) {
    if (!walletAccount || isLoadingMoreFriends) return;
    
    const container = document.getElementById('friendsList');
    
    try {
        if (!append) {
            friendsOffset = 0;
            hasMoreFriends = true;
            allFriends = [];
    container.innerHTML = '<div class="loading-state"><i class="fas fa-spinner fa-spin"></i> loading...</div>';
        }
        
        isLoadingMoreFriends = true;
        
        const response = await fetch(`${API_BASE}/users/${walletAccount}/friends?limit=${FOLLOW_USERS_PER_PAGE}&offset=${friendsOffset}`);
        const result = await response.json();
        
        if (result.success && result.data) {
            const newUsers = result.data;
            
            if (newUsers.length < FOLLOW_USERS_PER_PAGE) {
                hasMoreFriends = false;
            }
            
            if (append) {
                allFriends = allFriends.concat(newUsers);
        } else {
                allFriends = newUsers;
            }
            
            friendsOffset += newUsers.length;
            displayUsersList(allFriends, container, 'friends', hasMoreFriends);
        } else {
            if (!append) {
            container.innerHTML = '<div class="empty-state">No friends</div>';
            }
        }
    } catch (error) {
        console.error('Failed to load friends list:', error);
        if (!append) {
        container.innerHTML = '<div class="error-state">loading failed,please try again later/div>';
        }
    } finally {
        isLoadingMoreFriends = false;
    }
}


function displayUsersList(users, container, listType, hasMore = false) {
    if (users.length === 0) {
        container.innerHTML = `<div class="empty-state">Currently ${listType=='following'? 'Followed users': listType==' followers'? 'Fans':' Friends'}</div>`;
        return;
    }
    
    const usersHtml = users.map(user => {
        const displayName = user.username || `${user.ethereum_address.slice(0, 6)}...${user.ethereum_address.slice(-4)}`;
        const relationshipBadge = user.is_mutual ? 
            '<span class="relationship-badge mutual"><i class="fas fa-user-friends"></i> Currently</span>' :
            user.is_following ? 
                '<span class="relationship-badge following"><i class="fas fa-heart"></i> following</span>' :
                '<span class="relationship-badge follower"><i class="fas fa-users"></i> Fans</span>';
        
        return `
            <div class="user-card clickable-author" onclick="showUserProfile('${user.user_id || user.id}', '${user.ethereum_address || user.address || ''}')" data-user-id="${user.user_id || user.id}">
                <div class="user-avatar">
                    ${user.avatar ? `<img src="${user.avatar}" alt="profile picture">` : '<i class="fas fa-user"></i>'}
                </div>
                <div class="user-info">
                    <div class="user-name">${escapeHtml(displayName)} <i class="fas fa-external-link-alt"></i></div>
                    <div class="user-stats">
                        <span><i class="fas fa-edit"></i> ${user.posts_count} Posts</span>
                        <span><i class="fas fa-comment"></i> ${user.comments_count} comment</span>
                        <span><i class="fas fa-heart"></i> ${user.followers_count} fans</span>
                    </div>
                    ${user.bio ? `<div class="user-bio">${escapeHtml(user.bio)}</div>` : ''}
                </div>
                <div class="user-actions" onclick="event.stopPropagation()">
                    ${relationshipBadge}
                    ${generateFollowButton(user)}
                </div>
        </div>
    `;
    }).join('');
    
    // "Load More"
    const loadMoreButton = hasMore ? `
        <div class="load-more-container">
            <button class="load-more-btn" onclick="loadMore${listType.charAt(0).toUpperCase() + listType.slice(1)}()" ${getLoadingState(listType) ? 'disabled' : ''}>
                ${getLoadingState(listType) ? '<i class="fas fa-spinner fa-spin"></i> loading...' : `<i class="fas fa-chevron-down"></i> More loading${listType === 'following' ? 'follow' : listType === 'followers' ? '粉丝' : '朋友'}`}
            </button>
        </div>
    ` : '';
    
    container.innerHTML = usersHtml + loadMoreButton;
}


function getLoadingState(listType) {
    switch(listType) {
        case 'following': return isLoadingMoreFollowing;
        case 'followers': return isLoadingMoreFollowers;
        case 'friends': return isLoadingMoreFriends;
        default: return false;
    }
}


function generateFollowButton(user) {
    if (user.is_self) {
        return '<span class="self-badge">Me</span>';
    }
    
    const userId = user.user_id || user.id;
    
    if (user.is_following) {
        return `<button class="unfollow-btn" onclick="unfollowUserById('${userId}')">
            <i class="fas fa-user-minus"></i> unfollow
        </button>`;
    } else {
        return `<button class="follow-btn" onclick="followUserById('${userId}')">
            <i class="fas fa-user-plus"></i> follow
        </button>`;
    }
}


function generatePostAuthorFollowButton(post) {
    if (!currentUser.address || !post.author_id) {
        return ''; 
    }
    
    if (post.author_id === currentUser.id) {
        return '<span class="post-author-self-badge">Me</span>';
    }
    

    setTimeout(async () => {
        try {
            const isFollowing = await checkFollowStatusById(currentUser.id, post.author_id);
            const followButtonContainer = document.getElementById('postAuthorFollowButton');
            if (followButtonContainer) {
                if (isFollowing) {
                    followButtonContainer.innerHTML = `
                        <button class="post-unfollow-btn" onclick="unfollowUserFromPostById('${post.author_id}')">
                            <i class="fas fa-user-minus"></i> unfollow
                        </button>
                    `;
                } else {
                    followButtonContainer.innerHTML = `
                        <button class="post-follow-btn" onclick="followUserFromPostById('${post.author_id}')">
                            <i class="fas fa-user-plus"></i> follow
                        </button>
                    `;
                }
            }
        } catch (error) {
            console.error('Checking attention status failed:', error);
        }
    }, 100);
    
    return `<div id="postAuthorFollowButton" class="post-author-follow">
        <button class="post-follow-btn loading" disabled>
            <i class="fas fa-spinner fa-spin"></i> loading...
        </button>
    </div>`;
}

async function followUserById(targetUserId) {
    if (!currentUser.address) {
        showSuccessMessage('Operation failed', 'Please connect wallet first');
        return;
    }
    
    try {
        const button = document.querySelector(`[data-user-id="${targetUserId}"] .follow-btn`);
        if (button) {
            button.disabled = true;
            button.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Attention in progress...';
        }
        
        const response = await fetch(`${API_BASE}/follow`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                follower_id: currentUser.id,
                following_id: targetUserId
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
            showSuccessMessage('Follow Success!！', '🎉 You have successfully followed the user');
       
            await loadFollowStats();
            await refreshCurrentTab();
        } else {
            showSuccessMessage('Follows fails', result.error || 'Operation failed, please try again later');
        }
    } catch (error) {
        console.error('Failed to follow users:', error);
        showSuccessMessage('Follows fails', 'Network error, please try again later');
    }
}


async function followUser(targetAddress) {
    console.warn('Used the deprecated followUser function, please use followUserById');
 
}


async function unfollowUserById(targetUserId) {
    if (!currentUser.address) {
        showSuccessMessage('operation failed', 'Please connect the wallet first');
        return;
    }
    
    try {
        const button = document.querySelector(`[data-user-id="${targetUserId}"] .unfollow-btn`);
        if (button) {
            button.disabled = true;
            button.innerHTML = '<i class="fas fa-spinner fa-spin"></i> cancel中...';
        }
        
        const requestData = {
            follower_id: currentUser.id,
            following_id: targetUserId
        };
        console.log('Sending unfollow request (unfollowUserById):', requestData, 'currentUser:', currentUser);
        
        const response = await fetch(`${API_BASE}/unfollow`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(requestData)
        });
        
        const result = await response.json();
        
        if (result.success) {
            showSuccessMessage('cancelFollow Success!！', '✅ You have unfollowed this user');
         
            await loadFollowStats();
            await refreshCurrentTab();
        } else {
            showSuccessMessage('cancelFollows fails', result.error || 'Operation failed, please try again later');
        }
    } catch (error) {
        console.error('cancelFollows fails:', error);
        showSuccessMessage('cancelFollows fails', 'Network error, please try again later');
    }
}


async function unfollowUser(targetAddress) {
    console.warn('Used the deprecated unfollowUser function, please use unfollowUserById');
    
}


async function refreshCurrentTab() {
    const activeTab = document.querySelector('.tab-content.active');
    if (!activeTab) return;
    
    switch(activeTab.id) {
        case 'followingTab':
            await loadFollowingList();
            break;
        case 'followersTab':
            await loadFollowersList();
            break;
        case 'friendsTab':
            await loadFriendsList();
            break;
    }
}


function searchUsers() {
    const searchText = document.getElementById('followSearch').value.trim().toLowerCase();
    const userCards = document.querySelectorAll('#followingList .user-card');
    
    userCards.forEach(card => {
        const userName = card.querySelector('.user-name').textContent.toLowerCase();
        const userAddress = card.querySelector('.user-address').textContent.toLowerCase();
        
        if (userName.includes(searchText) || userAddress.includes(searchText)) {
            card.style.display = 'flex';
        } else {
            card.style.display = 'none';
        }
    });
}


async function checkFollowStatusById(followerId, followingId) {
    try {
        const response = await fetch(`${API_BASE}/follow/status?follower_id=${encodeURIComponent(followerId)}&following_id=${encodeURIComponent(followingId)}`);
        const result = await response.json();
        
        if (result.success && result.data) {
            return result.data.is_following;
        }
        return false;
    } catch (error) {
        console.error('Checking attention status failed:', error);
        return false;
    }
}


async function checkFollowStatus(followerAddress, followingAddress) {
    console.warn('Used the deprecated checkFolloweStatus function, please use checkFollowedStatusById ');
    return false;
}


async function followUserFromPostById(targetUserId) {
    if (!currentUser.address) {
        showSuccessMessage('operation failed', 'Please connect the wallet first');
        return;
    }
    
    try {
        const button = document.querySelector('#postAuthorFollowButton .post-follow-btn');
        if (button) {
            button.disabled = true;
            button.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Follow in progress...';
        }
        
        const response = await fetch(`${API_BASE}/follow`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                follower_id: currentUser.id,
                following_id: targetUserId
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
            showSuccessMessage('Follow Success!！', '🎉 You have successfully followed the user');
       
            const container = document.getElementById('postAuthorFollowButton');
            if (container) {
                container.innerHTML = `
                    <button class="post-unfollow-btn" onclick="unfollowUserFromPostById('${targetUserId}')">
                        <i class="fas fa-user-minus"></i> follow failed!
                    </button>
                `;
            }
        } else {
            showSuccessMessage('Follows fails', result.error || 'Operation failed, please try again later');
          
            if (button) {
                button.disabled = false;
                button.innerHTML = '<i class="fas fa-user-plus"></i> follow';
            }
        }
    } catch (error) {
        console.error('Failed to follow users:', error);
        showSuccessMessage('Follows fails', 'Network error, please try again later');
  
        const button = document.querySelector('#postAuthorFollowButton .post-follow-btn');
        if (button) {
            button.disabled = false;
            button.innerHTML = '<i class="fas fa-user-plus"></i> follow';
        }
    }
}


async function unfollowUserFromPostById(targetUserId) {
    if (!currentUser.address) {
        showSuccessMessage('operation failed', 'Please connect the wallet first');
        return;
    }
    
    try {
        const button = document.querySelector('#postAuthorFollowButton .post-unfollow-btn');
        if (button) {
            button.disabled = true;
            button.innerHTML = '<i class="fas fa-spinner fa-spin"></i> cancel中...';
        }
        
        const requestData = {
            follower_id: currentUser.id,
            following_id: targetUserId
        };
        console.log('Sending unfollow request (unfollowUserFromPostById):', requestData, 'currentUser:', currentUser);
        
        const response = await fetch(`${API_BASE}/unfollow`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify(requestData)
        });
        
        const result = await response.json();
        
        if (result.success) {
            showSuccessMessage('cancelFollow Success!！', '✅ You have unfollowed this user');
     
            const container = document.getElementById('postAuthorFollowButton');
            if (container) {
                container.innerHTML = `
                    <button class="post-follow-btn" onclick="followUserFromPostById('${targetUserId}')">
                        <i class="fas fa-user-plus"></i> follow
                    </button>
                `;
            }
        } else {
            showSuccessMessage('cancelFollows fails', result.error || 'Operation failed, please try again later');
          
            if (button) {
                button.disabled = false;
                button.innerHTML = '<i class="fas fa-user-minus"></i> cancelfollow';
            }
        }
    } catch (error) {
        console.error('cancelFollows fails:', error);
        showSuccessMessage('cancelFollows fails', 'Network error, please try again later');
     
        const button = document.querySelector('#postAuthorFollowButton .post-unfollow-btn');
        if (button) {
            button.disabled = false;
            button.innerHTML = '<i class="fas fa-user-minus"></i> cancelfollow';
        }
    }
}


function loadFriends() {
    const friendsSection = document.getElementById('friends');
    if (!friendsSection) return;
    
  
    if (!walletAccount || !currentUser.address) {
        friendsSection.innerHTML = `
            <div class="wallet-connect-prompt">
                <div class="wallet-prompt-card">
                    <div class="wallet-icon">
                        <img src="/icon/Gunman_Sprite.webp" alt="My friengs">
                    </div>
                    <h3>Connect wallet to view my friends</h3>
                    <p>Please connect your wallet first to view and manage your friend relationships</p>
                    <button class="connect-wallet-btn" onclick="connectWallet()">
                        <span class="wallet-btn-icon">🦊</span>
                        connect MetaMask wallet
                    </button>
                    <div class="wallet-tips">
                        <p>💡 After connecting the wallet, you can：</p>
                        <ul>
                            <li>View all your friends</li>
                            <li>Manage friendships</li>
                            <li>View friend data statistics</li>
                        </ul>
                    </div>
                </div>
            </div>
        `;
        return;
    }
    

    loadIrysContent();
 
    setTimeout(() => {
        const friendsTabBtn = document.querySelector('[onclick="switchFollowTab(\'friends\')"]');
        if (friendsTabBtn) {
            friendsTabBtn.click();
        }
    }, 100);
}


function toggleSidebar() {
    const sidebar = document.getElementById('rightSidebar');
    const overlay = document.getElementById('sidebarOverlay');
    
    if (sidebar && overlay) {
        const isShowing = sidebar.classList.contains('show');
        
        if (isShowing) {
            sidebar.classList.remove('show');
            overlay.classList.remove('show');
            document.body.style.overflow = 'auto';
        } else {
            sidebar.classList.add('show');
            overlay.classList.add('show');
            document.body.style.overflow = 'hidden';
        }
    }
}


window.addEventListener('resize', function() {
    if (window.innerWidth > 768) {
        const sidebar = document.getElementById('rightSidebar');
        const overlay = document.getElementById('sidebarOverlay');
        
        if (sidebar && overlay) {
            sidebar.classList.remove('show');
            overlay.classList.remove('show');
            document.body.style.overflow = 'auto';
        }
    }
});


function onUsernameInput() {
    const usernameInput = document.getElementById('usernameInput');
    const statusDiv = document.getElementById('usernameStatus');
    const mintBtn = document.querySelector('.mint-btn');
    
    if (!usernameInput || !statusDiv || !mintBtn) return;
    

    statusDiv.innerHTML = '';
    mintBtn.disabled = true;
    
    console.log('Username input changed:', usernameInput.value);
}


async function checkUsernameAvailability() {
    console.log('Checking username availability...');
    
    const usernameInput = document.getElementById('usernameInput');
    const statusDiv = document.getElementById('usernameStatus');
    const mintBtn = document.querySelector('.mint-btn');
    
    console.log('Elements found:', {
        usernameInput: !!usernameInput,
        statusDiv: !!statusDiv,
        mintBtn: !!mintBtn
    });
    
    if (!usernameInput || !statusDiv || !mintBtn) {
        console.error('Required elements not found');
        return;
    }
    
    const username = usernameInput.value.trim();
    
    if (!username) {
        statusDiv.innerHTML = '<span class="error">enter one user name</span>';
        mintBtn.disabled = true;
        return;
    }
    
    if (username.length < 2 || username.length > 20) {
        statusDiv.innerHTML = '<span class="error">The username length must be between 2-20 characters</span>';
        mintBtn.disabled = true;
        return;
    }
    
    
    if (!/^[\p{L}\p{N}_·]+$/u.test(username)) {
        statusDiv.innerHTML = '<span class="error">The username can contain Chinese characters, letters, numbers, underscores, or "·"</span>';
        mintBtn.disabled = true;
        return;
    }
    
    try {
        statusDiv.innerHTML = '<span class="checking">Checking...</span>';
        
        const response = await fetch(`${API_BASE}/username/check?username=${encodeURIComponent(username)}`);
        const result = await response.json();
        
        if (result.success && result.data) {
            if (result.data.available) {
                statusDiv.innerHTML = '<span class="success">✓ username availables/span>';
                mintBtn.disabled = false;
            } else {
                statusDiv.innerHTML = '<span class="error">✗ username not available</span>';
                mintBtn.disabled = true;
            }
        } else {
            statusDiv.innerHTML = '<span class="error">Check failed,please try again</span>';
            mintBtn.disabled = true;
        }
    } catch (error) {
        console.error('Error checking username:', error);
        statusDiv.innerHTML = '<span class="error">check failed,please try again</span>';
        mintBtn.disabled = true;
    }
}

//Register username
async function registerUsername() {
    console.log('Starting username registration...');
    
    const usernameInput = document.getElementById('usernameInput');
    const statusDiv = document.getElementById('usernameStatus');
    const mintBtn = document.querySelector('.mint-btn');
    
    console.log('Elements found:', {
        usernameInput: !!usernameInput,
        statusDiv: !!statusDiv,
        mintBtn: !!mintBtn,
        walletAccount: walletAccount
    });
    
    if (!usernameInput || !statusDiv || !mintBtn) {
        console.error('Required elements not found');
        return;
    }
    
    const username = usernameInput.value.trim();
    
    if (!username || mintBtn.disabled) {
        return;
    }
    
    try {
        mintBtn.disabled = true;
        mintBtn.textContent = 'register...';
        statusDiv.innerHTML = '<span class="processing">Calling smart contract...</span>';
        
   
        const contract = await getContract();
        if (!contract) {
            throw new Error('Unable to connect to smart contract');
        }
        
        const usernameCost = ethers.utils.parseEther('0.002'); // 0.002 ETH
        
        const tx = await contract.registerUsername(username, {
            value: usernameCost,
            gasLimit: 200000
        });
        
        statusDiv.innerHTML = '<span class="processing">Waiting for transaction confirmation...</span>';
        await tx.wait();
        
      
        statusDiv.innerHTML = '<span class="processing">Save username...</span>';
        
        const response = await fetch(`${API_BASE}/username/register`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                username: username,
                user_address: walletAccount
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
            showSuccessMessage('Username registration successful', `congratulations! Your username has been successfully registered`);
      
            loadUserProfile();
        } else {
          
            if (result.error && result.error.includes('Existing username')) {
         
                showSuccessMessage('The username already exists', `You have already registered your username, there is no need to register again`);
              
                loadUserProfile();
                return;
            }
            throw new Error(result.error || 'Backend save failed');
        }
        
    } catch (error) {
        console.error('Error registering username:', error);
        
        let errorMessage = 'Registration failed';
        if (error.code === 'ACTION_REJECTED') {
            errorMessage = 'User cancelled transaction';
        } else if (error.message.includes('Username already exists')) {
            errorMessage = 'Username already exists';
        } else if (error.message.includes('User already has a username')) {
            errorMessage = 'You already have a username';
        } else if (error.message.includes('Insufficient payment')) {
            errorMessage = 'Insufficient payment amount';
        } else if (error.message.includes('Invalid username format')) {
            errorMessage = 'Incorrect username format';
        }
        
        statusDiv.innerHTML = `<span class="error">✗ ${errorMessage}</span>`;
        showSuccessMessage('Registration failed', errorMessage);
        
        mintBtn.disabled = false;
        mintBtn.textContent = 'Mint username';
    }
}

// Check if user has username permission (for posting and commenting)
async function checkUserPermissions() {
    if (!walletAccount) {
        return { canPost: false, reason: 'Please connect the wallet first' };
    }
    
    try {
        const response = await fetch(`${API_BASE}/users/${walletAccount}/has-username`);
        const result = await response.json();
        
        if (result.success && result.data) {
            return { canPost: true };
        } else {
            return { canPost: false, reason: 'Please register a username first to post and comment' };
        }
    } catch (error) {
        console.error('Error checking user permissions:', error);
        return { canPost: false, reason: 'Check permission failed' };
    }
}

// Sync username status
async function syncUsername() {
    if (!walletAccount) {
        showWarningToast('Please connect the wallet first');
        return;
    }
    
    try {
        const response = await fetch(`${API_BASE}/username/sync`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                user_address: walletAccount
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
            showSuccessMessage('Synchronization successful', result.data || 'Username status synchronized');
           
            loadUserProfile();
        } else {
            showWarningToast(result.error || 'Synchronization failed');
        }
    } catch (error) {
        console.error('Error syncing username:', error);
        showWarningToast('Synchronization failed, please try again later');
    }
}

// Initialize app
async function initializeApp() {
   
    const savedAddress = localStorage.getItem('userAddress');
    const savedName = localStorage.getItem('userName');
    
    if (savedName) {
        currentUser.name = savedName;
    }
    
   
    await checkWalletConnection();
    
   
    document.querySelectorAll('.nav-tab').forEach(item => {
        item.addEventListener('click', function() {
            const view = this.dataset.view;
            switchView(view);
        });
    });
    

    if (typeof window.ethereum !== 'undefined') {
        window.ethereum.on('chainChanged', function(chainId) {
            console.log('Network switched to:', chainId);
            updateNetworkStatus();
            if (walletAccount) {
                updateBalance();
            }
        });
        
        window.ethereum.on('accountsChanged', function(accounts) {
            console.log('Accounts changed:', accounts);
            if (accounts.length === 0) {
                disconnectWallet();
                updateNetworkStatus(); 
            } else if (accounts[0] !== walletAccount) {
                walletAccount = accounts[0];
                currentUser.address = walletAccount;
                updateWalletUI();
                updateNetworkStatus(); 
            }
        });
    }
}


async function checkWalletConnection() {
    if (typeof window.ethereum !== 'undefined') {
        try {
            const accounts = await window.ethereum.request({ method: 'eth_accounts' });
            if (accounts.length > 0) {
               
                walletAccount = accounts[0];
                currentUser.address = walletAccount;
                updateWalletUI();
                await updateBalance();
                
               
                await fetchUserInfo();
                
               
                localStorage.setItem('userAddress', walletAccount);
                
                console.log('Wallet connected:', walletAccount);
            } else {
               
                walletAccount = null;
                currentUser.address = null;
                updateWalletUI();
                console.log('No connected wallet accounts');
            }
        } catch (error) {
            console.error('Check wallet connection failed:', error);
           
            walletAccount = null;
            currentUser.address = null;
            updateWalletUI();
        }
    }
}


function updateWalletUI() {
    const connectBtn = document.getElementById('connectWalletBtn');
    const connectedWallet = document.getElementById('walletConnected');
    const balanceSpan = document.getElementById('walletBalance');
    
    if (walletAccount) {
        if (connectBtn) connectBtn.style.display = 'none';
        if (connectedWallet) connectedWallet.style.display = 'flex';
        if (balanceSpan) {
           
            const currentBalance = balanceSpan.textContent.includes('IRYS') ? 
                balanceSpan.textContent.split('|')[1] || '0 IRYS' : 
                '0 IRYS';
            balanceSpan.textContent = `Address: ${walletAccount.substring(0, 6)}...${walletAccount.substring(38)} | ${currentBalance}`;
        }
    } else {
        if (connectBtn) connectBtn.style.display = 'block';
        if (connectedWallet) connectedWallet.style.display = 'none';
    }
}


function setupEventListeners() {
   
}


async function connectWallet() {
    try {
       
        if (typeof window.ethereum === 'undefined') {
                alert('Please install MetaMask wallet first');
            return;
        }

       
        const accounts = await window.ethereum.request({ 
            method: 'eth_requestAccounts' 
        });

        if (accounts.length > 0) {
            walletAccount = accounts[0];
            currentUser.address = walletAccount;
            
           
            await switchToIrysNetwork();
            
           
            const connectBtn = document.getElementById('connectWalletBtn');
            const connectedDiv = document.getElementById('walletConnected');
            const balanceSpan = document.getElementById('walletBalance');
            
            if (connectBtn) connectBtn.style.display = 'none';
            if (connectedDiv) connectedDiv.style.display = 'flex';
            if (balanceSpan) {
               
                const currentBalance = balanceSpan.textContent.includes('IRYS') ? 
                    balanceSpan.textContent.split(' ')[1] + ' ' + balanceSpan.textContent.split(' ')[2] : 
                    '0 IRYS';
                balanceSpan.textContent = `Address: ${walletAccount.substring(0, 6)}...${walletAccount.substring(38)} | ${currentBalance}`;
            }
            
           
            await updateBalance();
            
           
            await fetchUserInfo();
            
           
            localStorage.setItem('userAddress', walletAccount);
            
           
            const currentView = document.querySelector('.nav-tab.active')?.dataset.view;
            if (currentView === 'my-posts') {
                console.log('Wallet connected; refreshing My Posts view');
                loadMyPosts();
            } else if (currentView === 'posts' || !currentView) {
                console.log('Wallet connected; refreshing posts to show like status');
                loadPosts();
            }
            
            console.log('Wallet connection successful:', walletAccount);
        }
    } catch (error) {
        console.error('Connect wallet failed:', error);
        alert('Connect wallet failed: ' + error.message);
    }
}


async function fetchUserInfo() {
    if (!currentUser.address) {
        return;
    }
    
    try {
        const response = await fetch(`${API_BASE}/users/${currentUser.address}`);
        if (response.ok) {
            const result = await response.json();
            if (result.success && result.data) {
                currentUser.id = result.data.id;
                currentUser.name = result.data.name || '';
                console.log('Fetched user info successfully:', currentUser);
            }
        }
    } catch (error) {
        console.error('Get user information failed:', error);
    }
}


async function switchToIrysNetwork() {
    try {
       
        await window.ethereum.request({
            method: 'wallet_switchEthereumChain',
                    params: [{ chainId: '0x4F6' }], 
        });
    } catch (switchError) {
       
        if (switchError.code === 4902) {
            try {
                await window.ethereum.request({
                    method: 'wallet_addEthereumChain',
                    params: [{
                        chainId: '0x4F6', 
                        chainName: 'Irys Testnet',
                        nativeCurrency: {
                            name: 'IRYS',
                            symbol: 'IRYS',
                            decimals: 18
                        },
                        rpcUrls: ['https://testnet-rpc.irys.xyz', 'https://testnet.irys.xyz'],
                        blockExplorerUrls: ['https://explorer.irys.xyz']
                    }]
                });
            } catch (addError) {
                console.error('Add network failed:', addError);
                alert('Please manually add Irys testnet to MetaMask');
            }
        } else {
            console.error('Switch network failed:', switchError);
        }
    }
}


async function updateBalance() {
    try {
        const balance = await window.ethereum.request({
            method: 'eth_getBalance',
            params: [walletAccount, 'latest']
        });
        
        const balanceInIrys = parseInt(balance, 16) / Math.pow(10, 18);
        const balanceSpan = document.getElementById('walletBalance');
        
        if (balanceSpan && walletAccount) {
           
            const addressPart = `Address: ${walletAccount.substring(0, 6)}...${walletAccount.substring(38)}`;
            balanceSpan.textContent = `${addressPart} | Balance: ${balanceInIrys.toFixed(4)} IRYS`;
        }
    } catch (error) {
        console.error('Get balance failed:', error);
        const balanceSpan = document.getElementById('walletBalance');
        if (balanceSpan && walletAccount) {
            const addressPart = `Address: ${walletAccount.substring(0, 6)}...${walletAccount.substring(38)}`;
            balanceSpan.textContent = `${addressPart} | Balance: Get failed`;
        }
    }
}

function disconnectWallet() {
    walletAccount = null;
    currentUser.address = '';
    
   
    const connectBtn = document.getElementById('connectWalletBtn');
    const connectedDiv = document.getElementById('walletConnected');
    
    if (connectBtn) connectBtn.style.display = 'block';
    if (connectedDiv) connectedDiv.style.display = 'none';
    
   
    localStorage.removeItem('userAddress');
    
    console.log('Wallet disconnected');
}


function switchView(viewName) {
   
    document.querySelectorAll('.view-section').forEach(view => {
        view.style.display = 'none';
    });
    
   
    document.querySelectorAll('.nav-tab').forEach(item => {
        item.classList.remove('active');
    });
    
   
    const postComposeSection = document.querySelector('.post-compose-section');
    if (postComposeSection) {
        if (viewName === 'posts' || viewName === 'create') {

            postComposeSection.style.display = 'block';
        } else {
           
            postComposeSection.style.display = 'none';
        }
    }
    
   
    let targetViewId = viewName;
    if (viewName === 'create') {
       
        targetViewId = 'posts';
    }
    
    const targetView = document.getElementById(targetViewId);
    if (targetView) {
        targetView.style.display = 'block';
    } else {
        console.warn(`View ${targetViewId} does not exist`);
        return;
    }
    
   
    const navItem = document.querySelector(`.nav-tab[data-view="${viewName}"]`);
    if (navItem) {
        navItem.classList.add('active');
    }
    
   
    switch(viewName) {
        case 'posts':
            loadPosts();
            break;
        case 'profile':
            loadUserProfile();
            break;
        case 'my-posts':
            loadMyPosts();
            break;
        case 'irys':
            loadIrysContent();
            break;
                        case 'recommendations':
                    loadDailyRecommendations();
            break;
        case 'create':
           
            const titleInput = document.getElementById('postTitle');
            const contentInput = document.getElementById('postContent');
            
            if (titleInput) titleInput.value = '';
            if (contentInput) contentInput.value = '';
            clearImagePreview('postImagePreview');
            break;
        case 'profile':
            if (typeof loadUserProfile === 'function') {
                loadUserProfile();
            }
            break;
        case 'irys':
           
            break;
        case 'postDetail':
           
            break;
        case 'userProfile':
           
            break;
    }
}


async function loadPosts() {
   
    currentOffset = 0;
    hasMorePosts = true;
    
    const container = document.getElementById('postsContainer');
    container.innerHTML = '<div class="loading"><i class="fas fa-spinner fa-spin"></i> 加载中...</div>';
    
    try {
       
        let url = `${API_BASE}/posts?limit=${POSTS_PER_PAGE}&offset=${currentOffset}`;
        if (walletAccount) {
            url += `&user_address=${walletAccount}`;
           
        } else {
           
        }
        
        const response = await fetch(url);
        const result = await response.json();
        
        if (result.success && result.data) {
           
           
            
           
            hasMorePosts = result.data.length === POSTS_PER_PAGE;
            
                displayPosts(result.data, false); 
            
           
            currentOffset += result.data.length;
            
           
            showLoadMoreButton();
        } else {
            container.innerHTML = '<div class="error">Load posts failed</div>';
        }
    } catch (error) {
        console.error('Error loading posts:', error);
        container.innerHTML = '<div class="error">Network error, please try again later</div>';
    }
}


async function loadMorePosts() {
    if (isLoadingMore || !hasMorePosts) {
        return;
    }
    
    isLoadingMore = true;
    const loadMoreBtn = document.getElementById('loadMoreBtn');
    if (loadMoreBtn) {
        loadMoreBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> 加载中...';
        loadMoreBtn.disabled = true;
    }
    
    try {
       
        let url = `${API_BASE}/posts?limit=${POSTS_PER_PAGE}&offset=${currentOffset}`;
        if (walletAccount) {
            url += `&user_address=${walletAccount}`;
           
        } else {
           
        }
        
        const response = await fetch(url);
        const result = await response.json();
        
        if (result.success && result.data) {
           
            
           
            hasMorePosts = result.data.length === POSTS_PER_PAGE;
            
            if (result.data.length > 0) {
                displayPosts(result.data, true); 
                currentOffset += result.data.length;
            }
            
           
            updateLoadMoreButton();
        } else {
            console.error('Load more posts failed');
        }
    } catch (error) {
        console.error('Error loading more posts:', error);
    } finally {
        isLoadingMore = false;
    }
}


function displayPosts(posts, append = false) {
    const container = document.getElementById('postsContainer');
    
    if (posts.length === 0 && !append) {
        container.innerHTML = '<div class="empty-state">No posts yet, publish your first post now!</div>';
        return;
    }
    
   
    const uniquePosts = [];
    const seenIds = new Set();
    
   
    if (append) {
        const existingPosts = container.querySelectorAll('.post-item');
        existingPosts.forEach(element => {
            const postId = element.getAttribute('onclick')?.match(/openPost\('([^']+)'\)/)?.[1];
            if (postId) {
                seenIds.add(postId);
            }
        });
    }
    
    
    
    for (const post of posts) {
        if (!seenIds.has(post.id)) {
            seenIds.add(post.id);
            uniquePosts.push(post);
            
        } else {
            
        }
    }
    
    if (uniquePosts.length === 0) {
        
        return;
    }
    
    const postsHTML = uniquePosts.map(post => `
        <div class="post-item" onclick="openPost('${post.id}')">
            <div class="post-header">
                <div class="post-meta">
                    <div class="post-author clickable-author" onclick="event.stopPropagation(); showUserProfile('${post.author_id || ''}', '${post.author_address || ''}')" title="View user profile">
                        ${escapeHtml(post.author_name || (post.author_address ? `${post.author_address.slice(0, 6)}...${post.author_address.slice(-4)}` : 'Anonymous user'))}
                        <i class="fas fa-external-link-alt"></i>
                    </div>
                    <div class="post-time">${formatTime(post.created_at)}</div>
                </div>
            </div>
            <div class="post-content">
                <h4 class="post-title">${escapeHtml(post.title)}</h4>
                <p class="post-preview">${escapeHtml(post.content.substring(0, 150))}${post.content.length > 150 ? '...' : ''}</p>
                ${post.image ? `<div class="post-image-preview"><img src="${post.image}" alt="Post image" onclick="showImageModal('${post.image}')"></div>` : ''}
                </div>
            <div class="post-actions">
                <button class="comment-btn" onclick="event.stopPropagation(); openPost('${post.id}')">
                    <i class="fas fa-comment"></i>
                    <span>${post.comments_count}</span>
                </button>
                <button class="like-btn ${post.is_liked_by_user ? 'liked' : ''}" onclick="event.stopPropagation(); likePost('${post.id}', this)" data-post-id="${post.id}">
                    ${post.is_liked_by_user ? '<img src="/icon/Group_1073717789.webp" alt="已点赞" class="like-icon">' : '<i class="far fa-heart"></i>'}
                    <span class="like-count">${post.likes}</span>
                </button>
            </div>
        </div>
    `).join('');
    
    if (append) {
        
        container.insertAdjacentHTML('beforeend', postsHTML);
    } else {
        
    container.innerHTML = postsHTML;
    }
    
    
}


function showLoadMoreButton() {
    const container = document.getElementById('postsContainer');
    const existingBtn = document.getElementById('loadMoreBtn');
    
    
    if (existingBtn) {
        existingBtn.remove();
    }
    
    
    if (hasMorePosts) {
        const loadMoreBtn = document.createElement('div');
        loadMoreBtn.className = 'load-more-container';
        loadMoreBtn.innerHTML = `
            <button id="loadMoreBtn" class="load-more-btn" onclick="loadMorePosts()">
                <i class="fas fa-plus"></i> Load More Posts
            </button>
        `;
        container.parentNode.insertBefore(loadMoreBtn, container.nextSibling);
    }
}


function updateLoadMoreButton() {
    const loadMoreBtn = document.getElementById('loadMoreBtn');
    if (!loadMoreBtn) return;
    
    if (hasMorePosts) {
        loadMoreBtn.innerHTML = '<i class="fas fa-plus"></i> Load More Posts';
        loadMoreBtn.disabled = false;
    } else {
        loadMoreBtn.innerHTML = '<i class="fas fa-check"></i> No more posts';
        loadMoreBtn.disabled = true;
        
        
        setTimeout(() => {
            const container = loadMoreBtn.parentElement;
            if (container) {
                container.remove();
            }
        }, 3000);
    }
}


let isPosting = false; 
let lastPostContent = ''; 
let lastPostTime = 0; 
let postInProgress = false; 

async function createPost() {
   
    if (postInProgress) {
        showSuccessMessage('Posting', 'Please wait for the current post to complete');
        return;
    }
    postInProgress = true;
    
    
    if (isPosting) {
        postInProgress = false;
        showSuccessMessage('Posting', 'Please wait for the current post to complete');
        return;
    }
    
    const contentInput = document.getElementById('postContent');
    if (!contentInput) {
        showSuccessMessage('Post failed', 'Cannot find the post content input box, please refresh the page and try again');
        return;
    }
    const content = contentInput.value.trim();
    
    const now = Date.now();
    if (content === lastPostContent && (now - lastPostTime) < 5000) { 
        showSuccessMessage('Post failed', 'Please do not post the same content again');
        return;
    }
    
    console.log('Start creating post...'); 
    console.log('Content:', content);
    console.log('Current user:', currentUser);
    
    if (!content) {
        showSuccessMessage('Post failed', 'Please fill in the content');
        return;
    }
    
    const tagRegex = /#([a-zA-Z0-9\u4e00-\u9fa5]+)/g;
    const tags = [];
    let match;
    while ((match = tagRegex.exec(content)) !== null) {
        const tag = match[1].trim();
        if (tag && tag.length > 0 && tag.length <= 20) { 
            tags.push(tag);
        }
    }
    
    const title = content.length > 30 ? content.substring(0, 30) + '...' : content;
    
    if (!currentUser.address) {
        showSuccessMessage('Post failed', 'Please connect the wallet first');
        return;
    }
    
    const permissions = await checkUserPermissions();
    if (!permissions.canPost) {
        showWarningToast(permissions.reason);
        return;
    }

    isPosting = true;
    isSubmittingPost = true;
    
    const postBtn = document.querySelector('.create-post-btn, .post-btn, .submit-btn');
    const originalPostBtnText = postBtn ? postBtn.innerHTML : '';
    if (postBtn) {
        postBtn.disabled = true;
        postBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Posting...';
    }
    
    lastPostContent = content;
    lastPostTime = now;
    
    const postData = {
        title,
        content,
        author_address: currentUser.address,
        author_name: currentUser.name || null,
        tags,
        image: currentPostImage
    };
    
    console.log('📤 Payload:', {
        ...postData,
        image: currentPostImage ? `image data (${currentPostImage.length} chars)` : 'no image'
    }); 
    
    const startTime = performanceMonitor.startRequest();
    
    try {
        await requestQueue.add(async () => {
            console.log('🔍 Check smart contract call conditions...');
            console.log('ethers object:', typeof ethers);
            console.log('Wallet connection status:', !!window.ethereum);
            console.log('Current account:', walletAccount);
        
        if (!window.ethereum) {
            showSuccessMessage('Post failed', 'Please install MetaMask wallet');
            return;
        }
        
        if (!walletAccount) {
            showSuccessMessage('Post failed', 'Please connect the wallet first');
            return;
        }
        
        
        const chainId = await window.ethereum.request({ method: 'eth_chainId' });
        console.log('Current network:', chainId);
        if (chainId !== '0x4f6') {
            showSuccessMessage('Post failed', 'Please switch to Irys testnet');
            return;
        }
        
        if (!isPosting) {
            console.log('⚠️ Post status has been reset, cancel this post');
            return;
        }
        
        console.log('📡 Start calling smart contract to create post...');
        
        try {
            const { pointsEarned, transactionHash, blockchainPostId } = await createPostOnChain(
                postData.title,
                postData.content,
                postData.tags,
                'mock_tx_id' 
            );
            
            console.log('✅ Smart contract call successful, transaction hash:', transactionHash);
            console.log('✅ Smart contract post ID:', blockchainPostId);
            console.log('✅ Now save to backend database...');
            
            
            const response = await fetch(`${API_BASE}/posts`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    ...postData,
                    blockchain_post_id: blockchainPostId, 
                    blockchain_transaction_hash: transactionHash 
                })
            });
            
            console.log('Response status:', response.status);
            
            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }
            
            const result = await response.json();
            console.log('Response result:', result);
            
            if (result.success) {
                showSuccessMessage(
                    'Post published successfully!', 
                    `🎉 Smart contract call successful! Get ${pointsEarned} points reward!`
                );
                
                document.getElementById('postContent').value = '';
                document.getElementById('postImage').value = '';
                clearImagePreview('postImagePreview');
                await loadPosts();
            } else {
                showSuccessMessage('Post failed', 'Backend save failed: ' + (result.error || 'Unknown error'));
            }
            
            } catch (contractError) {
                console.error('Smart contract call failed:', contractError);
                showSuccessMessage('Post failed', 'Need to pay Irys fee to post');
                throw contractError;
            }
        });
        
        performanceMonitor.endRequest(startTime, true);
        
    } catch (error) {
        console.error('Error creating post:', error);
        performanceMonitor.endRequest(startTime, false);
        
        if (error.message.includes('User denied transaction')) {
            showSuccessMessage('Transaction canceled', 'You canceled the transaction, the post was not published');
        } else {
            showSuccessMessage('Post failed', 'Network error: ' + error.message);
        }
    } finally {
        
        isPosting = false;
        postInProgress = false;
        isSubmittingPost = false;
        
        const postBtn = document.querySelector('.create-post-btn, .post-btn, .submit-btn');
        if (postBtn) {
            postBtn.disabled = false;
            postBtn.innerHTML = originalPostBtnText || '<i class="fas fa-paper-plane"></i> Post';
        }
    }
}


async function openPost(postId) {
    currentPostId = postId;
    
    try {
        const url = walletAccount ? 
            `${API_BASE}/posts/${postId}?user_address=${encodeURIComponent(walletAccount)}` : 
            `${API_BASE}/posts/${postId}`;
        
        const response = await fetch(url);
        const result = await response.json();
        
        if (result.success && result.data) {
            const post = result.data;
            
            window.currentPostData = post;
            
            switchView('postDetail');
            
            const container = document.getElementById('postDetailContainer');
            if (container) {
                container.innerHTML = `
                    <div class="post-detail-header">
                        <button class="back-btn" onclick="switchView('posts')">
                            <i class="fas fa-arrow-left"></i> Back
                        </button>
                        <div class="post-detail-meta">
                            <div class="post-author">
                                <div class="post-author-avatar">
                                    ${post.author_avatar ? `<img src="${post.author_avatar}" alt="Avatar">` : '<i class="fas fa-user"></i>'}
                                </div>
                                <div class="post-author-info">
                                    <div class="post-author-name clickable-author" onclick="showUserProfile('${post.author_id || ''}', '${post.author_address || ''}')" title="View user profile">
                                        ${escapeHtml(post.author_name || (post.author_address ? `${post.author_address.slice(0, 6)}...${post.author_address.slice(-4)}` : 'Anonymous user'))}
                                        <i class="fas fa-external-link-alt"></i>
                                    </div>
                                    <div class="post-time">${formatTime(post.created_at)}</div>
                                </div>
                                ${generatePostAuthorFollowButton(post)}
                            </div>
                        </div>
                    </div>
                    
                    <div class="post-detail-content">
                        <div class="post-content">${escapeHtml(post.content)}</div>
                        ${post.image ? `<img src="${post.image}" alt="Post image" class="post-image">` : ''}
                        ${post.tags && post.tags.length > 0 ? `
                            <div class="post-tags">
                                ${post.tags.map(tag => `<span class="tag">#${escapeHtml(tag)}</span>`).join('')}
                            </div>
                        ` : ''}
                        
                        <div class="post-actions">
                            <div class="post-stats">
                                                <span class="post-stat like-btn ${post.is_liked_by_user ? 'liked' : ''}" onclick="likePost('${post.id}', this)" data-post-id="${post.id}">
                    ${post.is_liked_by_user ? '<img src="/icon/Group_1073717789.webp" alt="Liked" class="like-icon">' : '<i class="far fa-heart"></i>'} <span class="like-count">${post.likes || 0}</span>
                </span>
                                <span class="post-stat">
                                    <i class="fas fa-comment"></i> ${post.comments_count || 0}
                                </span>
                            </div>
                        </div>
                    </div>
                    
                    <div class="comments-section">
                        <h3>Comments</h3>
                        <div class="comment-form">
                            <textarea id="commentContent" placeholder="Write your comment..." rows="3"></textarea>
                            <div class="comment-actions">
                                <div class="media-upload">
                                    <input type="file" id="commentImage" accept="image/jpeg,image/jpg,image/png" style="display: none;" onchange="handleImageUpload(this, 'commentImagePreview')">
                                    <button class="media-btn" onclick="document.getElementById('commentImage').click()">
                                        <i class="fas fa-image"></i> Image
                                    </button>
                                    <div id="commentImagePreview" class="image-preview-compact"></div>
                                </div>
                                <button class="post-btn" onclick="addComment()">
                                    <i class="fas fa-comment"></i> Post comment
                                </button>
                            </div>
                        </div>
                        <div class="comments-list" id="commentsList">
                        </div>
                    </div>
                `;
            }
            
            await loadComments(postId);
        } else {
            alert('Failed to load post');
        }
    } catch (error) {
        console.error('Error loading post:', error);
        alert('Network error, please try again later');
    }
}

let commentsOffset = 0;
const COMMENTS_PER_PAGE = 20;
let isLoadingMoreComments = false;
let hasMoreComments = true;
let allComments = []; 

async function loadComments(postId, append = false) {
    if (isLoadingMoreComments) return;
    
    try {
        if (!append) {
            commentsOffset = 0;
            hasMoreComments = true;
            allComments = [];
        }
        
        isLoadingMoreComments = true;
        
        let url = `${API_BASE}/posts/${postId}/comments?limit=${COMMENTS_PER_PAGE}&offset=${commentsOffset}`;
        if (walletAccount) {
            url += `&user_address=${encodeURIComponent(walletAccount)}`;
        }
        
        const response = await fetch(url);
        const result = await response.json();
        
        if (result.success && result.data) {
            const newComments = result.data;
            
            if (newComments.length < COMMENTS_PER_PAGE) {
                hasMoreComments = false;
            }
            
            if (append) {
                allComments = allComments.concat(newComments);
        } else {
                allComments = newComments;
            }
            
            commentsOffset += newComments.length;
            displayComments(allComments, !append);
        } else {
            if (!append) {
            document.getElementById('commentsList').innerHTML = '<div class="empty-state">No comments</div>';
            }
        }
    } catch (error) {
        console.error('Failed to load comments:', error);
        if (!append) {
        document.getElementById('commentsList').innerHTML = '<div class="empty-state">Failed to load comments</div>';
        }
    } finally {
        isLoadingMoreComments = false;
    }
}

// Display comment list
function displayComments(comments, showLoadMore = true) {
    const container = document.getElementById('commentsList');
    
    if (comments.length === 0) {
        container.innerHTML = '<div class="empty-state">No comments, please post the first comment</div>';
        return;
    }
    
    const commentTree = buildCommentTree(comments);
    
    const commentsHTML = commentTree.map(comment => renderComment(comment, 0)).join('');
    
    const loadMoreButton = hasMoreComments && showLoadMore ? `
        <div class="load-more-container">
            <button class="load-more-btn" onclick="loadMoreComments()" ${isLoadingMoreComments ? 'disabled' : ''}>
                ${isLoadingMoreComments ? '<i class="fas fa-spinner fa-spin"></i> Loading...' : '<i class="fas fa-chevron-down"></i> Load more comments'}
            </button>
        </div>
    ` : '';
    
    container.innerHTML = commentsHTML + loadMoreButton;
}

// Load more comments
async function loadMoreComments() {
    if (!currentPostId || !hasMoreComments || isLoadingMoreComments) return;
    
    await loadComments(currentPostId, true);
}

// Build comment tree structure
function buildCommentTree(comments) {
    const commentMap = new Map();
    const rootComments = [];
    
    // Create all comment mappings first
    comments.forEach(comment => {
        comment.replies = [];
        commentMap.set(comment.id, comment);
    });
    
    // Build tree structure
    comments.forEach(comment => {
        if (comment.parent_id && commentMap.has(comment.parent_id)) {
            // Sub-comment
            commentMap.get(comment.parent_id).replies.push(comment);
        } else {
            // Root comment
            rootComments.push(comment);
        }
    });
    
    return rootComments;
}

// Render single comment (supports nesting)
function renderComment(comment, level = 0) {
    const indent = level * 20; // Each level is indented by 20px
    const maxLevel = 3; // Maximum nesting level
    const isMaxLevel = level >= maxLevel;
    
    return `
        <div class="comment-item ${level > 0 ? 'comment-reply' : ''}" style="margin-left: ${indent}px;" data-comment-id="${comment.id}">
            <div class="comment-header">
                <div class="comment-author-avatar">
                    ${comment.author_avatar ? `<img src="${comment.author_avatar}" alt="Avatar">` : '<i class="fas fa-user"></i>'}
                </div>
                <div class="comment-author-info">
                    <div class="comment-author-name clickable-author" onclick="showUserProfile('${comment.author_id || ''}', '${comment.author_address || ''}')" title="View user profile">
                        ${escapeHtml(comment.author_name || (comment.author_address ? `${comment.author_address.slice(0, 6)}...${comment.author_address.slice(-4)}` : 'Anonymous user'))}
                        <i class="fas fa-external-link-alt"></i>
                    </div>
                    <div class="comment-time">${formatTime(comment.created_at)}</div>
                </div>
                <div class="comment-actions">
                    ${!isMaxLevel ? `<button class="reply-btn" onclick="showReplyForm('${comment.id}', '${escapeHtml(comment.author_name || 'Anonymous user')}')">
                        <i class="fas fa-reply"></i> Reply
                    </button>` : ''}
                    <button class="like-btn ${comment.is_liked_by_user ? 'liked' : ''}" onclick="likeComment('${comment.id}')">
                        ${comment.is_liked_by_user ? '<img src="/icon/Group_1073717789.webp" alt="Liked" class="like-icon">' : '<i class="far fa-heart"></i>'} ${comment.likes || 0}
                    </button>
                </div>
            </div>
            <div class="comment-content">${escapeHtml(comment.content)}</div>
            ${comment.image ? `<img src="${comment.image}" alt="Comment image" class="comment-image" onclick="showImageModal('${comment.image}')">` : ''}
            
            <!-- Reply form container -->
            <div class="reply-form-container" id="replyForm-${comment.id}" style="display: none;">
                <div class="reply-form">
                    <div class="reply-to">Reply @${escapeHtml(comment.author_name || 'Anonymous user')}</div>
                    <textarea placeholder="Write your reply..." rows="3" id="replyContent-${comment.id}"></textarea>
                    <div class="reply-actions">
                        <button class="cancel-btn" onclick="hideReplyForm('${comment.id}')">cancel</button>
                        <button class="submit-btn" onclick="submitReply('${comment.id}')">
                            <i class="fas fa-paper-plane"></i> Send reply
                        </button>
        </div>
                </div>
            </div>
            
            <!-- Render reply -->
            ${comment.replies && comment.replies.length > 0 ? comment.replies.map(reply => renderComment(reply, level + 1)).join('') : ''}
        </div>
    `;
}

// Display reply form
function showReplyForm(commentId, authorName) {
    // Hide other reply forms
    document.querySelectorAll('.reply-form-container').forEach(form => {
        form.style.display = 'none';
    });
    
    // Display current reply form
    const replyForm = document.getElementById(`replyForm-${commentId}`);
    if (replyForm) {
        replyForm.style.display = 'block';
        // Focus on text box
        const textarea = document.getElementById(`replyContent-${commentId}`);
        if (textarea) {
            textarea.focus();
        }
    }
}

// Hide reply form
function hideReplyForm(commentId) {
    const replyForm = document.getElementById(`replyForm-${commentId}`);
    if (replyForm) {
        replyForm.style.display = 'none';
        // Clear content
        const textarea = document.getElementById(`replyContent-${commentId}`);
        if (textarea) {
            textarea.value = '';
        }
    }
}

// Submit reply
async function submitReply(parentCommentId) {
    const content = document.getElementById(`replyContent-${parentCommentId}`).value.trim();
    
    if (!content) {
        showSuccessMessage('Reply failed', 'Please enter reply content');
        return;
    }
    
    if (!currentUser.address) {
        showSuccessMessage('Reply failed', 'Please connect the wallet first');
        return;
    }
    
    if (!currentPostId) {
        showSuccessMessage('Reply failed', 'Please select a post first');
        return;
    }
    
    // Set submission status
    const submitBtn = document.querySelector(`#replyForm-${parentCommentId} .submit-btn`);
    const originalText = submitBtn ? submitBtn.innerHTML : '';
    if (submitBtn) {
        submitBtn.disabled = true;
        submitBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Sending...';
    }
    
    try {
        // Call smart contract first
        let transactionHash = null;
        try {
            if (window.ethereum && walletAccount) {
                transactionHash = await createCommentOnChain(currentPostId, content, parentCommentId);
                console.log('💎 Smart contract transaction hash:', transactionHash);
            }
        } catch (contractError) {
            console.error('Smart contract call failed:', contractError);
            throw new Error('Smart contract call failed: ' + contractError.message);
        }
        
        // Call backend API
        const response = await fetch(`${API_BASE}/posts/${currentPostId}/comments`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                post_id: currentPostId, 
                content: content,
                author_address: currentUser.address,
                author_name: currentUser.name,
                parent_id: parentCommentId, 
                blockchain_transaction_hash: transactionHash
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
            showSuccessMessage('Reply published successfully!', '🎉 Congratulations on getting 30 points reward!');
            // Hide reply form
            hideReplyForm(parentCommentId);
            // Reload comment list
            await loadComments(currentPostId);
            
           
            setTimeout(() => {
                const newReply = document.querySelector(`[data-comment-id="${result.data?.id}"]`);
                if (newReply) {
                    newReply.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    newReply.style.backgroundColor = '#e6f3ff';
                    newReply.style.border = '2px solid #4CAF50';
                    newReply.style.borderRadius = '8px';
                    
                  
                    setTimeout(() => {
                        newReply.style.backgroundColor = '';
                        newReply.style.border = '';
                        newReply.style.borderRadius = '';
                    }, 3000);
                }
            }, 500);
        } else {
            throw new Error(result.message || 'Reply failed');
        }
    } catch (error) {
        console.error('❌ Publish reply failed:', error);
        showSuccessMessage('Reply failed', error.message || 'Network error, please try again later');
    } finally {
        // Restore button state
        if (submitBtn) {
            submitBtn.disabled = false;
            submitBtn.innerHTML = originalText;
        }
    }
}

// Like comment
async function likeComment(commentId) {
    try {
        const response = await fetch(`${API_BASE}/comments/${commentId}/like`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                user_address: currentUser.address
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
       
            const likeBtn = document.querySelector(`[data-comment-id="${commentId}"] .like-btn`);
            if (likeBtn) {
                const action = result.data.action;
                const likes = result.data.likes || 0;
                
                if (action === 'like') {
               
                    likeBtn.innerHTML = `<img src="/icon/Group_1073717789.webp" alt="Liked" class="like-icon"> ${likes}`;
                    likeBtn.classList.add('liked');
                    
                    // 点赞动画效果
                    likeBtn.style.transform = 'scale(1.3)';
                    setTimeout(() => {
                        likeBtn.style.transform = 'scale(1)';
                    }, 200);
                    
                    showSuccessMessage('Liked successfully!', '❤️ Thank you for your support!');
                } else {
                    // cancel like: empty heart
                    likeBtn.innerHTML = `<i class="far fa-heart"></i> ${likes}`;
                    likeBtn.classList.remove('liked');
                    
                    // cancel like animation effect
                    likeBtn.style.transform = 'scale(0.9)';
                    setTimeout(() => {
                        likeBtn.style.transform = 'scale(1)';
                    }, 200);
                    
                    showSuccessMessage('Canceled like', '💔 Hope to get your support next time');
                }
            }
        } else {
            showSuccessMessage('operation failed', result.message || 'Please try again later');
        }
    } catch (error) {
        console.error('❌ Like comment failed:', error);
        showSuccessMessage('Like failed', 'Network error, please try again later');
    }
}

// Add comment
async function addComment() {
    // Prevent duplicate submission
    if (isSubmittingComment) {
        console.log('⚠️ Comment is being submitted, please do not click again');
        return;
    }
    
    const content = document.getElementById('commentContent').value.trim();
    
    if (!content) {
        showSuccessMessage('Comment failed', 'Please enter comment content');
        return;
    }
    
    if (!currentUser.address) {
        showSuccessMessage('Comment failed', 'Please enter wallet address first');
        return;
    }
    
    if (!currentPostId) {
        showSuccessMessage('Comment failed', 'Please select a post first');
        return;
    }
    

    isSubmittingComment = true;
    const commentBtn = document.querySelector('.add-comment-btn');
    const originalBtnText = commentBtn ? commentBtn.innerHTML : '';
    if (commentBtn) {
        commentBtn.disabled = true;
        commentBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> Submitting...';
    }
    
    try {

        if (!checkEthers()) {
            showSuccessMessage('Comment failed', 'ethers.js not loaded, please refresh the page');
            return;
        }
        
        if (!walletAccount) {
            showSuccessMessage('Comment failed', 'Please connect the wallet first');
            return;
        }
        
   
    const permissions = await checkUserPermissions();
    if (!permissions.canPost) {
        showWarningToast(permissions.reason);
        return;
    }
        
     
        const chainId = await window.ethereum.request({ method: 'eth_chainId' });
        if (chainId !== '0x4f6') {
            showSuccessMessage('Comment failed', 'Please switch to Irys testnet');
            return;
        }
        
     
        console.log('📡 Start calling smart contract to create comment...');
        
        try {
            // Use current post ID to call smart contract
            const commentTxHash = await createCommentOnChain(currentPostId, content, null);
            
            console.log('✅ Smart contract call successful, transaction hash:', commentTxHash);
            console.log('✅ Now save to backend database...');
            
            // After smart contract is successful, save to backend database
            const response = await fetch(`${API_BASE}/posts/${currentPostId}/comments`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    post_id: currentPostId,
                    content,
                    author_address: currentUser.address,
                    author_name: currentUser.name || null,
                    image: currentCommentImage, 
                    blockchain_transaction_hash: commentTxHash 
                })
            });
            
            const result = await response.json();
            
            if (result.success) {
                showSuccessMessage('Comment published successfully!', '🎉 Congratulations on getting 50 points reward!');
                const newCommentId = result.data?.id; 
                document.getElementById('commentContent').value = '';
                document.getElementById('commentImage').value = '';
                clearImagePreview('commentImagePreview');
        
                await loadComments(currentPostId);
                
        
                setTimeout(() => {
                    const commentsContainer = document.getElementById('commentsList');
                    if (commentsContainer) {
                
                        commentsContainer.scrollIntoView({ behavior: 'smooth', block: 'start' });
                        
                     
                        const commentItems = commentsContainer.querySelectorAll('.comment-item');
                        if (commentItems.length > 0) {
                            const firstComment = commentItems[0]; 
                            firstComment.style.backgroundColor = '#e6f3ff';
                            firstComment.style.border = '2px solid #4CAF50';
                            firstComment.style.borderRadius = '8px';
                            
                        
                            setTimeout(() => {
                                firstComment.style.backgroundColor = '';
                                firstComment.style.border = '';
                                firstComment.style.borderRadius = '';
                            }, 3000);
                        }
                        
                        console.log('🎯 Automatically scroll to new comment position and highlight display');
                    }
                }, 500); 
            } else {
                showSuccessMessage('Comment failed', 'Backend save failed: ' + (result.error || 'Unknown error'));
            }
            
        } catch (contractError) {
            console.error('Smart contract call failed:', contractError);
            showSuccessMessage('Comment failed', 'Need to pay IRYS fee to comment');
            return;
        }
    } catch (error) {
        console.error('Error adding comment:', error);
        showSuccessMessage('Comment failed', 'Network error, please try again later');
    } finally {
        // Restore submission status and button
        isSubmittingComment = false;
        const commentBtn = document.querySelector('.add-comment-btn');
        if (commentBtn) {
            commentBtn.disabled = false;
            commentBtn.innerHTML = originalBtnText || '<i class="fas fa-paper-plane"></i> 发布评论';
        }
    }
}

// Load user profile
async function loadUserProfile() {
    const profileSection = document.getElementById('profile');
    if (!profileSection) return;

    if (!walletAccount) {
        profileSection.innerHTML = `
            <div class="wallet-connect-prompt">
                <div class="wallet-prompt-card">
                    <div class="wallet-icon">
                        <img src="/icon/Intellectual_Sprite.webp" alt="User profile">
                    </div>
                    <h3>Connect wallet to view user profile</h3>
                    <p>Please connect your wallet to view and manage your profile</p>
                    <button class="connect-wallet-btn" onclick="connectWallet()">
                        <span class="wallet-btn-icon">🦊</span>
                        Connect MetaMask wallet
                    </button>
                    <div class="wallet-tips">
                        <p>💡 After connecting your wallet, you can:</p>
                        <ul>
                            <li>View and edit your profile</li>
                            <li>Register a unique username</li>
                            <li>View your data statistics</li>
                        </ul>
                    </div>
                </div>
            </div>
        `;
        return;
    }

    try {
       
        const userResponse = await fetch(`${API_BASE}/users/${walletAccount}`);
        const userResult = await userResponse.json();
        const userStats = userResult.success && userResult.data ? userResult.data : {
            posts_count: 0,
            comments_count: 0,
            reputation: 0
        };


        const usernameResponse = await fetch(`${API_BASE}/users/${walletAccount}/username`);
        const usernameResult = await usernameResponse.json();
        const username = usernameResult.success && usernameResult.data ? usernameResult.data : null;


        const profileAddress = document.getElementById('profileAddress');
        const profileStats = document.getElementById('profileStats');
        const usernameSection = document.querySelector('.username-section');
        
 
        if (profileAddress) {
            if (username) {
                profileAddress.textContent = username;
                profileAddress.className = 'user-address username-display';
            } else {
                profileAddress.textContent = walletAccount;
                profileAddress.className = 'user-address';
            }
        }
        
        if (profileStats) {
            profileStats.innerHTML = `
                <span>Posts: ${userStats.posts_count}</span>
                <span>Comments: ${userStats.comments_count}</span>
                <span>Reputation: ${userStats.reputation}</span>
            `;
        }


        if (username) {
            
            if (usernameSection) {
                usernameSection.style.display = 'none';
            }
        } else {
  
            if (usernameSection) {
                usernameSection.style.display = 'block';
            }
            
            const usernameInput = document.getElementById('usernameInput');
            const mintBtn = document.querySelector('.mint-btn');
            const statusDiv = document.getElementById('usernameStatus');
            
            if (usernameInput) {
                usernameInput.disabled = false;
                usernameInput.value = '';
            }
            
            if (mintBtn) {
                mintBtn.textContent = 'Mint';
                mintBtn.disabled = true;
            }
            
            if (statusDiv) {
                statusDiv.innerHTML = '';
            }
        }

        // Load user avatar and bio
        loadUserAvatarAndBio(userStats);

    } catch (error) {
        console.error('Error loading user profile:', error);
        showWarningToast('Load user profile failed');
    }
}

// Query Irys data
async function queryIrys() {
    const address = document.getElementById('irysAddress').value.trim();
    const tagsInput = document.getElementById('irysTags').value.trim();
    const limit = document.getElementById('irysLimit').value;
    
    const tags = tagsInput ? tagsInput.split(',').map(tag => tag.trim()).filter(tag => tag) : null;
    
    try {
        const params = new URLSearchParams();
        if (address) params.append('address', address);
        if (tags) params.append('tags', JSON.stringify(tags));
        if (limit) params.append('limit', limit);
        
        const response = await fetch(`${API_BASE}/irys/query?${params}`);
        const result = await response.json();
        
        if (result.success) {
            displayIrysResults(result.data);
        } else {
            alert('Query failed: ' + (result.error || 'Unknown error'));
        }
    } catch (error) {
        console.error('Error querying Irys:', error);
        alert('Network error, please try again later');
    }
}

// Display Irys query results
function displayIrysResults(data) {
    const container = document.getElementById('irysResults');
    
    if (data.length === 0) {
        container.innerHTML = '<div class="empty-state">No related data found</div>';
        return;
    }
    
    const resultsHTML = data.map(item => `
        <div class="irys-item">
            <h4>Transaction ID: ${item.id || 'Unknown'}</h4>
            <pre>${JSON.stringify(item, null, 2)}</pre>
        </div>
    `).join('');
    
    container.innerHTML = resultsHTML;
}

// Refresh post list
function refreshPosts() {
    loadPosts();
}

// Close modal
function closeModal() {
    document.getElementById('postModal').classList.remove('active');
    currentPostId = null;
}

// Get current post data
function getCurrentPostData() {
    // Get post data from page DOM
    const postDetailElement = document.querySelector('.post-detail');
    if (postDetailElement) {
        const postId = postDetailElement.getAttribute('data-post-id');
        const blockchainPostId = postDetailElement.getAttribute('data-blockchain-post-id');
        
        if (blockchainPostId) {
            return {
                id: postId,
                blockchain_post_id: parseInt(blockchainPostId, 10)
            };
        }
    }
    
    // If there is no data in the DOM, try to get it from the global variable
    if (window.currentPostData && window.currentPostData.blockchain_post_id) {
        return window.currentPostData;
    }
    
    return null;
}

// Display image modal
function showImageModal(imageUrl) {
    // Create image modal
    const modal = document.createElement('div');
    modal.className = 'image-modal';
    modal.innerHTML = `
        <div class="image-modal-content">
            <span class="image-modal-close">&times;</span>
            <img src="${imageUrl}" alt="Image preview" class="image-modal-img">
        </div>
    `;
    
    // Add to page
    document.body.appendChild(modal);
    
    // Add close event
    modal.addEventListener('click', function(e) {
        if (e.target === modal || e.target.className === 'image-modal-close') {
            document.body.removeChild(modal);
        }
    });
    
    // ESC key close
    const handleEscape = function(e) {
        if (e.key === 'Escape') {
            document.body.removeChild(modal);
            document.removeEventListener('keydown', handleEscape);
        }
    };
    document.addEventListener('keydown', handleEscape);
}

// Utility functions
function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function truncateText(text, maxLength) {
    if (text.length <= maxLength) {
        return text;
    }
    return text.substring(0, maxLength) + '...';
}

function formatTime(timestamp) {
    const date = new Date(timestamp);
    const now = new Date();
    const diff = now - date;
    
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);
    const days = Math.floor(diff / 86400000);
    
    if (minutes < 1) {
        return 'Just now';
    } else if (minutes < 60) {
        return `${minutes} minutes ago`;
    } else if (hours < 24) {
        return `${hours} hours ago`;
    } else if (days < 30) {
        return `${days} days ago`;
    } else {
        return date.toLocaleDateString();
    }
}

// Click modal outside to close (if modal exists)
const postModal = document.getElementById('postModal');
if (postModal) {
    postModal.addEventListener('click', function(e) {
        if (e.target === this) {
            closeModal();
        }
    });
}


document.addEventListener('keydown', function(e) {
    if (e.key === 'Escape') {
        closeModal();
    }
});


function showWarningToast(message) {
 
    const toast = document.createElement('div');
    toast.className = 'warning-toast';
    toast.innerHTML = `
        <div class="toast-content">
            <i class="fas fa-exclamation-triangle"></i>
            <span>${message}</span>
        </div>
    `;
    

    document.body.appendChild(toast);
    

    setTimeout(() => toast.classList.add('show'), 100);
    
   
    setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => {
            if (document.body.contains(toast)) {
                document.body.removeChild(toast);
            }
        }, 300);
    }, 3000);
}


function showSuccessMessage(title, subtitle) {

    const messageDiv = document.createElement('div');
    messageDiv.className = 'success-message';
    messageDiv.innerHTML = `
        <div class="success-content">
            <div class="success-title">${title}</div>
            <div class="success-subtitle">${subtitle}</div>
        </div>
    `;
    
    // Add to page
    document.body.appendChild(messageDiv);
    
    // Show animation
    setTimeout(() => messageDiv.classList.add('show'), 100);
    
    // 3 seconds later automatically disappear
    setTimeout(() => {
        messageDiv.classList.remove('show');
        setTimeout(() => document.body.removeChild(messageDiv), 300);
    }, 3000);
}

// Check if ethers is available
function checkEthers() {
    const available = typeof ethers !== 'undefined';
    if (!available) {
        console.error('ethers.js library not loaded');
    }
    return available;
}

// Image upload processing function
function handleImageUpload(input, previewId) {
    console.log('🖼️ Start processing image upload:', { previewId, files: input.files.length });
    
    const file = input.files[0];
    const preview = document.getElementById(previewId);
    
    if (!file) {
        console.log('❌ No file selected');
        clearImagePreview(previewId);
        return;
    }
    
    console.log('📁 File information:', { 
        name: file.name, 
        type: file.type, 
        size: file.size,
        sizeKB: Math.round(file.size / 1024)
    });
    
    // Whitelist validation - only allow jpg and png
    const allowedTypes = ['image/jpeg', 'image/jpg', 'image/png'];
    if (!allowedTypes.includes(file.type)) {
        console.log('❌ File type not supported:', file.type);
        showSuccessMessage('Image format error', 'Only JPG and PNG images are supported');
        input.value = '';
        clearImagePreview(previewId);
        return;
    }
    
    // Size limit - 200KB
    const maxSize = 200 * 1024; // 200KB
    if (file.size > maxSize) {
        console.log('❌ File too large:', file.size, 'bytes');
        showSuccessMessage('File too large', 'File size cannot exceed 200KB');
        input.value = '';
        clearImagePreview(previewId);
        return;
    }
    
    console.log('✅ File validation passed, start reading...');
    
    // Read and preview image
    const reader = new FileReader();
    reader.onload = function(e) {
        const imageData = e.target.result;
        console.log('📷 Image read completed, data length:', imageData.length);
        
        showImagePreview(preview, imageData, previewId);
        
        // 保存图片数据到全局变量
        if (previewId === 'postImagePreview') {
            currentPostImage = imageData;
            console.log('💾 Post image saved to global variable');
        } else if (previewId === 'commentImagePreview') {
            currentCommentImage = imageData;
            console.log('💾 Comment image saved to global variable');
        }
    };
    
    reader.onerror = function(e) {
        console.error('❌ Image read failed:', e);
        showSuccessMessage('Image read failed', 'Please select a new image');
    };
    
    reader.readAsDataURL(file);
}

// Display image preview
function showImagePreview(preview, imageData, previewId) {
    if (previewId === 'postImagePreview' || previewId === 'commentImagePreview') {
        // Compact preview
        preview.className = 'image-preview-compact has-image';
        preview.innerHTML = `
            <img src="${imageData}" alt="Image preview">
            <button class="remove-image" onclick="removeImage('${previewId}')" title="Delete image">
                <i class="fas fa-times"></i>
            </button>
        `;
    } else {
      
        preview.className = 'image-preview has-image';
        preview.innerHTML = `
            <img src="${imageData}" alt="Image preview">
            <button class="remove-image" onclick="removeImage('${previewId}')">
                <i class="fas fa-trash"></i> Delete image
            </button>
        `;
    }
}


function clearImagePreview(previewId) {
    const preview = document.getElementById(previewId);
    if (previewId === 'postImagePreview' || previewId === 'commentImagePreview') {
  
        preview.className = 'image-preview-compact';
        preview.innerHTML = '';
        preview.style.display = 'none';
    } else {
      
        preview.className = 'image-preview';
        preview.innerHTML = '<div class="placeholder-text">Select an image to preview here</div>';
    }
    
  
    if (previewId === 'postImagePreview') {
        currentPostImage = null;
    } else if (previewId === 'commentImagePreview') {
        currentCommentImage = null;
    }
}

// Delete image
function removeImage(previewId) {
    // Clear file input box
    const inputId = previewId === 'postImagePreview' ? 'postImage' : 'commentImage';
    document.getElementById(inputId).value = '';
    
    // Clear preview
    clearImagePreview(previewId);
}

// Initialize image preview areas
function initializeImagePreviews() {
    console.log('🖼️ Initialize image preview area...');
    

    const postPreview = document.getElementById('postImagePreview');
    const postInput = document.getElementById('postImage');
    
    console.log('Post image element:', { preview: !!postPreview, input: !!postInput });
    
    if (postPreview) {
        postPreview.className = 'image-preview-compact';
        postPreview.style.display = 'none';
    }
    

    const commentPreview = document.getElementById('commentImagePreview');
    const commentInput = document.getElementById('commentImage');
    
    console.log('Comment image element:', { preview: !!commentPreview, input: !!commentInput });
    
    if (commentPreview) {
        commentPreview.className = 'image-preview-compact';
        commentPreview.style.display = 'none';
    }
}


async function getContract() {
    if (!checkEthers()) {
        throw new Error('ethers.js library not loaded, please refresh the page and try again');
    }
    
    if (!window.ethereum || !walletAccount) {
        throw new Error('Please connect the wallet first');
    }
    
    const provider = new ethers.providers.Web3Provider(window.ethereum);
    const signer = provider.getSigner();
    return new ethers.Contract(CONTRACT_ADDRESS, CONTRACT_ABI, signer);
}


async function getPostCost() {
    try {
        const contract = await getContract();
        const cost = await contract.postCost();
        return cost;
    } catch (error) {
        console.error('Get post cost failed:', error);
        return ethers.utils.parseEther('0.001');
    }
}


async function getCommentCost() {
    try {
        const contract = await getContract();
        const cost = await contract.commentCost();
        return cost;
    } catch (error) {
        console.error('Get comment cost failed:', error);
        return ethers.utils.parseEther('0.0005'); 
    }
}


async function createPostOnChain(title, content, tags, irysTransactionId) {
    try {
        console.log('🔗 Prepare to call smart contract to create post...');
        
        const contract = await getContract();
        const postCost = await getPostCost();
        
        console.log('💰 Post cost:', ethers.utils.formatEther(postCost), 'IRYS');
        
        const tx = await contract.createPost(title, content, tags, irysTransactionId, {
            value: postCost,
            gasLimit: 500000
        });
        
        console.log('📝 Transaction sent:', tx.hash);
        console.log('⏳ Waiting for transaction confirmation...');
        
        const receipt = await tx.wait();
        console.log('✅ Transaction confirmed:', receipt.transactionHash);
        
     
        const postCreatedEvent = receipt.events?.find(e => e.event === 'PostCreated');
        const pointsEarnedEvent = receipt.events?.find(e => e.event === 'PointsEarned');
        
        let blockchainPostId = null;
        if (postCreatedEvent) {
            blockchainPostId = postCreatedEvent.args.postId.toString();
            console.log('🎉 Post created successfully! Post ID:', blockchainPostId);
        }
        
        if (pointsEarnedEvent) {
            console.log('🏆 Get points:', pointsEarnedEvent.args.points.toString());
            return {
                pointsEarned: pointsEarnedEvent.args.points.toString(),
                transactionHash: receipt.transactionHash,
                blockchainPostId: blockchainPostId
            };
        }
        
        return {
            pointsEarned: '100',
            transactionHash: receipt.transactionHash,
            blockchainPostId: blockchainPostId
        };
    } catch (error) {
        console.error('❌ Smart contract call failed:', error);
        throw error;
    }
}


async function createCommentOnChain(postId, content, parentCommentId = null) {
    console.log('🔗 Prepare to call smart contract to create comment...');
    
    try {
        const contract = await getContract();
        console.log('🔧 Contract instance:', contract);
        console.log('🔧 Contract address:', contract.address);
        console.log('🔧 Contract interface:', contract.interface);
        
        if (contract.interface && contract.interface.functions) {
            console.log('🔧 Available functions:', Object.keys(contract.interface.functions));
        } else if (contract.functions) {
            console.log('🔧 Available functions:', Object.keys(contract.functions));
        } else {
            console.log('🔧 Unable to get contract function list');
        }
        
      
        if (typeof contract.createComment === 'function') {
            console.log('✅ createComment function exists');
        } else {
            console.log('❌ createComment function does not exist');
            console.log('🔧 Contract available methods:', Object.getOwnPropertyNames(contract));
        }
        
        const commentCost = await getCommentCost();
        
        console.log('💰 Comment cost:', ethers.utils.formatEther(commentCost), 'IRYS');
        console.log('📝 Comment content:', content);
        console.log('📍 Post ID:', postId);
        console.log('👨‍👩‍👧‍👦 Parent comment ID:', parentCommentId || 'None (top-level comment)');
        
        // Get current post's smart contract ID
        let postIdNumber = 1; 
        
        // Try to get blockchain_post_id from current post data
        const currentPostData = getCurrentPostData();
        if (currentPostData && currentPostData.blockchain_post_id) {
            postIdNumber = currentPostData.blockchain_post_id;
            console.log('🔗 Use smart contract post ID:', postIdNumber);
        } else {
            console.warn('⚠️ Unable to find smart contract post ID, using default value:', postIdNumber);
        }
        
        // Convert parent comment ID to number (smart contract needs)
        let parentIdNumber = 0;
        if (parentCommentId) {
            // UUID to number conversion function
            function uuidToNumber(uuid) {
                if (!uuid || typeof uuid !== 'string') return 0;
                const cleanUuid = uuid.replace(/-/g, '');
                const hex = cleanUuid.substring(0, 8);
                return parseInt(hex, 16) % 999999 + 1;
            }
            parentIdNumber = uuidToNumber(parentCommentId);
        }
        
        console.log('🔢 Converted post ID:', postIdNumber, '(original:', postId, ')');
        console.log('🔢 Converted parent comment ID:', parentIdNumber, '(original:', parentCommentId, ')');
        
        // Call smart contract to create comment
        console.log('📤 Send transaction to smart contract...');
        const tx = await contract.createComment(
            postIdNumber, // Use converted number ID
            content,
            parentIdNumber, // Use converted parent comment ID
            '', // irysTransactionId - can be empty or use backend returned ID
            {
                value: commentCost,
                gasLimit: 300000
            }
        );
        
        console.log('⏳ Waiting for transaction confirmation...', tx.hash);
        const receipt = await tx.wait();
        console.log('✅ Transaction confirmed:', receipt);
        
        // Extract points earned from event
        const commentCreatedEvent = receipt.logs.find(log => {
            try {
                const parsed = contract.interface.parseLog(log);
                return parsed && parsed.name === 'CommentCreated';
            } catch (e) {
                return false;
            }
        });
        
        if (commentCreatedEvent) {
            const parsed = contract.interface.parseLog(commentCreatedEvent);
            console.log('🏆 Get points:', parsed.args.pointsEarned.toString());
        }
        
        // Return transaction hash instead of points, points information can be obtained from backend
        return receipt.transactionHash;
    } catch (error) {
        console.error('❌ Smart contract call failed:', error);
        throw error;
    }
}

    // Update network status
async function updateNetworkStatus() {
    console.log('Start checking network status...');
    const networkStatus = document.getElementById('networkStatus');
    const networkName = document.getElementById('networkName');
    
    if (!networkStatus) {
        console.error('Unable to find networkStatus element');
        return;
    }
    
    if (typeof window.ethereum === 'undefined') {
        console.log('MetaMask not installed, set offline status');
        if (networkName) networkName.textContent = 'MetaMask not installed';
        networkStatus.className = 'status-dot offline';
        console.log('Set className to:', networkStatus.className);
        return;
    }
    
    try {
        // First check if wallet is connected
        const accounts = await window.ethereum.request({ method: 'eth_accounts' });
        console.log('Wallet accounts:', accounts);
        
        if (!accounts || accounts.length === 0) {
            console.log('Wallet not connected, set offline status');
            if (networkName) networkName.textContent = 'Wallet not connected';
            networkStatus.className = 'status-dot offline';
            console.log('Set className to:', networkStatus.className);
            const switchBtn = document.getElementById('switchNetworkBtn');
            if (switchBtn) switchBtn.style.display = 'none';
            return;
        }
        
        const chainId = await window.ethereum.request({ method: 'eth_chainId' });
        const networkStatus = document.getElementById('networkStatus');
        const networkName = document.getElementById('networkName');
        const networkContainer = document.querySelector('.network-status-bottom');
        const switchBtn = document.getElementById('switchNetworkBtn');
        
        console.log('Current chain ID:', chainId); // Debug use
        
        if (chainId === '0x4f6' || chainId === '0x4F6' || parseInt(chainId, 16) === 1270) { // Irys testnet (1270)
            console.log('Connected to Irys testnet and wallet is connected, set online status');
            if (networkName) networkName.textContent = 'IrysForum';
            networkStatus.className = 'status-dot online';
            if (networkContainer) networkContainer.classList.add('connected');
            console.log('Set className to:', networkStatus.className);
            if (switchBtn) switchBtn.style.display = 'none';
        } else {
            console.log('Not connected to IrysForum, set offline status, current chain ID:', chainId);
            if (networkName) networkName.textContent = `Not connected to IrysForum (current: ${parseInt(chainId, 16)})`;
            networkStatus.className = 'status-dot offline';
            if (networkContainer) networkContainer.classList.remove('connected');
            console.log('Set className to:', networkStatus.className);
            if (switchBtn) switchBtn.style.display = 'block';
        }
    } catch (error) {
        console.error('Get network status failed:', error);
        console.log('Network detection failed, set offline status:', error);
        if (networkName) networkName.textContent = 'Network detection failed';
        networkStatus.className = 'status-dot offline';
        const networkContainer = document.querySelector('.network-status-bottom');
        if (networkContainer) networkContainer.classList.remove('connected');
        console.log('Set className to:', networkStatus.className);
    }
}

// Load global statistics data
async function loadGlobalStats() {
    try {
        const response = await fetch(`${API_BASE}/stats/global`);
        const result = await response.json();
        
        if (result.success) {
            const stats = result.data;
            const totalUsers = document.getElementById('totalUsers');
            const totalPosts = document.getElementById('totalPosts');
            const totalComments = document.getElementById('totalComments');
            const totalLikes = document.getElementById('totalLikes');
            
            if (totalUsers) totalUsers.textContent = stats.total_users.toLocaleString();
            if (totalPosts) totalPosts.textContent = stats.total_posts.toLocaleString();
            if (totalComments) totalComments.textContent = stats.total_comments.toLocaleString();
            if (totalLikes) totalLikes.textContent = stats.total_likes.toLocaleString();
            
            console.log('📊 Global statistics loaded:', stats);
        }
    } catch (error) {
        console.error('❌ Load global statistics failed:', error);
    }
}

// Load active users ranking
async function loadActiveUsers() {
    try {
        const response = await fetch(`${API_BASE}/stats/active-users?limit=10`);
        const result = await response.json();
        
        if (result.success && result.data.length > 0) {
            displayActiveUsers(result.data);
            console.log('👑 Active users ranking loaded:', result.data.length, 'users');
        } else {
            const container = document.getElementById('activeUsersList');
            if (container) {
                container.innerHTML = `
                    <div style="text-align: center; padding: 40px 20px; color: #6b7280;">
                        <i class="fas fa-users" style="font-size: 32px; margin-bottom: 12px; opacity: 0.5;"></i>
                        <p>No active users data</p>
                        <p style="font-size: 12px;">Will appear in the ranking after posting a post or comment</p>
                    </div>
                `;
            }
        }
    } catch (error) {
        console.error('❌ Load active users failed:', error);
        const container = document.getElementById('activeUsersList');
        if (container) {
            container.innerHTML = `
                <div style="text-align: center; padding: 40px 20px; color: #ef4444;">
                    <i class="fas fa-exclamation-triangle" style="font-size: 32px; margin-bottom: 12px;"></i>
                    <p>Load ranking failed</p>
                </div>
            `;
        }
    }
}

// Display active users ranking
function displayActiveUsers(users) {
    const container = document.getElementById('activeUsersList');
    if (!container) return;
    
    // Only show the top 10 (double insurance, backend and frontend jointly limit)
    const topTen = (users || []).slice(0, 10);
    const html = topTen.map((user, index) => {
        const rank = index + 1;
        const rankClass = rank === 1 ? 'gold' : rank === 2 ? 'silver' : rank === 3 ? 'bronze' : '';
        
            // Display user name (prefer backend's name, then username, then address alias)
        const displayName = (user.name || user.username || `user_${(user.address || '').slice(2, 8)}`).toString();
        const bio = user.bio ? String(user.bio) : '';
        
        // Calculate activity score (keep logic, reserve for extension)
        const activityScore = user.reputation || (user.posts_count * 10 + user.comments_count * 5);
        
        return `
            <div class="user-rank-item clickable-author" onclick="showUserProfile('${user.id || ''}', '${user.ethereum_address || user.address || ''}')" title="${bio ? escapeHtml(bio) : 'View user profile'}">
                <div class="rank-number ${rankClass}">${rank}</div>
                <div class="user-rank-info">
                    <div class="user-rank-name">${escapeHtml(displayName)}</div>
                    <div class="user-rank-stats">${user.posts_count} posts • ${user.comments_count} comments${bio ? ` • <span class="user-rank-bio">${escapeHtml(bio.substring(0, 18))}${bio.length > 18 ? '…' : ''}</span>` : ''}</div>
                </div>
            </div>
        `;
    }).join('');
    
    container.innerHTML = html;
}

// Like post
async function likePost(postId, element) {
    // Prevent event bubbling, avoid triggering openPost
    event.stopPropagation();
    
    if (!walletAccount) {
        showSuccessMessage('Like failed', 'Please connect the wallet first');
        return;
    }
    
    try {
        // Disable button, prevent duplicate clicks
        element.style.pointerEvents = 'none';
        element.style.opacity = '0.6';
        
        const response = await fetch(`${API_BASE}/posts/${postId}/like`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                user_address: walletAccount
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
            // Switch like status visual effect
            const newLikeCount = result.data;
            
            // Check different page formats
            const isDetailPage = element.classList.contains('post-stat');
            const isProfilePage = element.classList.contains('like-btn-card');
            
            if (element.classList.contains('liked')) {
                // Cancel like: image -> empty heart
                if (isDetailPage) {
                    element.innerHTML = `<i class="far fa-heart"></i> <span class="like-count">${newLikeCount}</span>`;
                } else if (isProfilePage) {
                    element.innerHTML = `<i class="far fa-heart"></i>
                        <span>${newLikeCount}</span>`;
                } else {
                    element.innerHTML = `<i class="far fa-heart"></i>
                        <span class="like-count">${newLikeCount}</span>`;
                }
                element.classList.remove('liked');
            } else {
                // Like: empty heart -> image
                if (isDetailPage) {
                    element.innerHTML = `<img src="/icon/Group_1073717789.webp" alt="Liked" class="like-icon"> <span class="like-count">${newLikeCount}</span>`;
                } else if (isProfilePage) {
                    element.innerHTML = `<img src="/icon/Group_1073717789.webp" alt="Liked" class="like-icon">
                        <span>${newLikeCount}</span>`;
                } else {
                    element.innerHTML = `<img src="/icon/Group_1073717789.webp" alt="Liked" class="like-icon">
                        <span class="like-count">${newLikeCount}</span>`;
                }
                element.classList.add('liked');
                
                // Add like animation effect
                element.style.transform = 'scale(1.2)';
                setTimeout(() => {
                    element.style.transform = 'scale(1)';
                }, 200);
            }
            
            console.log('👍 Like success:', postId, 'New like count:', result.data);
        } else {
            showSuccessMessage('Like failed', result.error || 'operation failed');
        }
    } catch (error) {
        console.error('❌ Like failed:', error);
        showSuccessMessage('Like failed', 'Network error');
    } finally {
        // Restore button status
        element.style.pointerEvents = 'auto';
        element.style.opacity = '1';
    }
}

// ========== User profile functionality ==========

// Display user profile
async function showUserProfile(userId, userAddress) {
    console.log('🔍 Display user profile called:', { userId, userAddress });
    
    // Add debug information
    if (!userId && !userAddress) {
        console.error('❌ User ID and address are empty, cannot display user profile');
        alert('Unable to get user information, please try again later');
        return;
    }
    
    // Switch to user profile view
    switchView('userProfile');
    
    // Display loading status
    const container = document.getElementById('userProfileContainer');
    container.innerHTML = `
        <div class="loading-container">
            <i class="fas fa-spinner fa-spin"></i>
            <span>Loading user profile...</span>
        </div>
    `;
    
    try {
        // Load user information and posts
        await loadUserProfileData(userId, userAddress);
    } catch (error) {
        console.error('Load user profile failed:', error);
        container.innerHTML = `
            <div class="error-container">
                <i class="fas fa-exclamation-triangle"></i>
                <p>Load user profile failed</p>
                <button class="retry-btn" onclick="showUserProfile('${userId}', '${userAddress}')">Retry</button>
                <button class="back-btn" onclick="switchView('posts')">Back</button>
            </div>
        `;
    }
}

// Load user profile data
async function loadUserProfileData(userId, userAddress) {
    const container = document.getElementById('userProfileContainer');
    
    try {
        // Parallel load user information, posts and follow data
        const [userInfoResponse, userPostsResponse, followStatsResponse] = await Promise.all([
            // Get user basic information by address (currently using this API, can be changed to get by ID later)
            fetch(`${API_BASE}/users/${userAddress}`).catch(() => null),
            // Get user's posts (including current user's like status)
            (() => {
                const url = `${API_BASE}/users/${userAddress}/posts?limit=20&offset=0${walletAccount ? `&user_address=${encodeURIComponent(walletAccount)}` : ''}`;
                console.log('🔍 User profile API request URL:', url, 'Current wallet:', walletAccount);
                return fetch(url);
            })(),
            // Get follow statistics
            fetch(`${API_BASE}/users/${userAddress}/follow-stats`).catch(() => null)
        ]);
        
        // Parse response
        const userInfo = userInfoResponse?.ok ? await userInfoResponse.json() : null;
        const userPostsData = userPostsResponse.ok ? await userPostsResponse.json() : { data: [] };
        const followStats = followStatsResponse?.ok ? await followStatsResponse.json() : { data: { following_count: 0, followers_count: 0, mutual_follows_count: 0 } };
        
        // Debug user posts data
        console.log('🔍 User posts data:', userPostsData.data?.slice(0, 2)?.map(post => ({
            id: post.id,
            title: post.title,
            is_liked_by_user: post.is_liked_by_user,
            likes: post.likes
        })));
        
        // Render user profile
        renderUserProfile({
            userId,
            userAddress,
            userInfo: userInfo?.data,
            posts: userPostsData.data || [],
            followStats: followStats.data || { following_count: 0, followers_count: 0, mutual_follows_count: 0 }
        });
        
    } catch (error) {
        console.error('Load user data failed:', error);
        throw error;
    }
}

// Render user profile
function renderUserProfile({ userId, userAddress, userInfo, posts, followStats }) {
    const container = document.getElementById('userProfileContainer');
    
    // Determine displayed user name - prefer user name
    const displayName = userInfo?.name || userInfo?.username || userInfo?.author_name || 
                       (userAddress ? `User${userAddress.slice(-4)}` : 'Anonymous user');
    const displayBio = userInfo?.bio || '';
    
    // Check if it is the current user himself
    const isSelf = currentUser.address && userAddress && 
                   currentUser.address.toLowerCase() === userAddress.toLowerCase();
    
    container.innerHTML = `
        <div class="user-profile-header">
            <button class="back-btn" onclick="switchView('posts')">
                <i class="fas fa-arrow-left"></i> Back
            </button>
            
            <div class="user-profile-info">
                <div class="user-avatar-large">
                    ${userInfo?.avatar ? `<img src="${userInfo.avatar}" alt="User avatar">` : '<i class="fas fa-user"></i>'}
                </div>
                
                <div class="user-details">
                    <h2 class="user-name">${escapeHtml(displayName)}</h2>
                    ${isSelf ? '<span class="self-badge">This is you</span>' : ''}
                    ${displayBio ? `<div class="user-bio-line">${escapeHtml(displayBio)}</div>` : ''}
                    
                    <div class="user-stats-simple">
                        <span class="stat-text">Posts: ${posts.length}</span>
                        <span class="stat-text">Followers: ${followStats.followers_count || 0}</span>
                        <span class="stat-text">Following: ${followStats.following_count || 0}</span>
                    </div>
                    
                    ${!isSelf && currentUser.address ? generateUserProfileFollowButton(userId, userAddress) : ''}
                </div>
            </div>
        </div>
        
        <div class="user-profile-content">
            <div class="profile-section">
                <h3 class="section-title">
                    <i class="fas fa-edit"></i>
                    User posts (${posts.length})
                </h3>
                
                ${posts.length > 0 ? `
                    <div class="user-posts-grid">
                        ${posts.map(post => `
                            <article class="user-post-card" onclick="openPost('${post.id}')">
                                <div class="post-card-header">
                                    <h4 class="post-title">${escapeHtml(post.title || 'No title')}</h4>
                                    <time class="post-date">${formatTime(post.created_at)}</time>
                                </div>
                                <div class="post-card-content">
                                    ${escapeHtml(post.content.substring(0, 120))}${post.content.length > 120 ? '...' : ''}
                                </div>
                                <div class="post-card-footer">
                                    <div class="post-card-stats">
                                                        <span class="stat-item like-btn-card ${post.is_liked_by_user ? 'liked' : ''}" onclick="event.stopPropagation(); likePost('${post.id}', this)" data-post-id="${post.id}">
                    ${post.is_liked_by_user ? '<img src="/icon/Group_1073717789.webp" alt="Liked" class="like-icon">' : '<i class="far fa-heart"></i>'}
                    <span>${post.likes || 0}</span>
                                        </span>
                                        <span class="stat-item">
                                            <i class="fas fa-comment"></i>
                                            <span>${post.comments_count || 0}</span>
                                        </span>
                                    </div>
                                    ${post.tags && post.tags.length > 0 ? `
                                        <div class="post-tags">
                                            ${post.tags.slice(0, 2).map(tag => `<span class="tag">#${escapeHtml(tag)}</span>`).join('')}
                                            ${post.tags.length > 2 ? '<span class="tag-more">...</span>' : ''}
                                        </div>
                                    ` : ''}
                                </div>
                            </article>
                        `).join('')}
                    </div>
                ` : `
                    <div class="empty-posts">
                        <i class="fas fa-inbox"></i>
                        <p>This user has not posted any posts</p>
                        ${isSelf ? '<button class="create-post-btn" onclick="switchView(\'posts\')">Create post now</button>' : ''}
                    </div>
                `}
            </div>
        </div>
    `;
    
    // If not self, asynchronously load follow status
    if (!isSelf && currentUser.address && userId) {
        loadUserProfileFollowStatus(userId);
    }
}

// Generate follow button for user profile
function generateUserProfileFollowButton(userId, userAddress) {
    return `
        <div id="userProfileFollowButton" class="user-profile-follow">
            <button class="follow-btn loading" disabled>
                <i class="fas fa-spinner fa-spin"></i> Checking...
            </button>
        </div>
    `;
}

// Load follow status for user profile
async function loadUserProfileFollowStatus(userId) {
    try {
        const isFollowing = await checkFollowStatusById(currentUser.id, userId);
        const followButtonContainer = document.getElementById('userProfileFollowButton');
        
        if (followButtonContainer) {
            if (isFollowing) {
                followButtonContainer.innerHTML = `
                    <button class="unfollow-btn" onclick="unfollowUserFromProfile('${userId}')">
                        <i class="fas fa-user-minus"></i> cancelfollow
                    </button>
                `;
            } else {
                followButtonContainer.innerHTML = `
                    <button class="follow-btn" onclick="followUserFromProfile('${userId}')">
                        <i class="fas fa-user-plus"></i> follow
                    </button>
                `;
            }
        }
    } catch (error) {
        console.error('Checking attention status failed:', error);
        const followButtonContainer = document.getElementById('userProfileFollowButton');
        if (followButtonContainer) {
            followButtonContainer.innerHTML = `
                <button class="follow-btn" onclick="followUserFromProfile('${userId}')">
                    <i class="fas fa-user-plus"></i> follow
                </button>
            `;
        }
    }
}

    // Follow user from user profile
async function followUserFromProfile(targetUserId) {
    if (!currentUser.address || !currentUser.id) {
        showSuccessMessage('operation failed', 'Please connect the wallet first');
        return;
    }
    
    try {
        const button = document.querySelector('#userProfileFollowButton .follow-btn');
        if (button) {
            button.disabled = true;
            button.innerHTML = '<i class="fas fa-spinner fa-spin"></i> following...';
        }
        
        const success = await followUserById(targetUserId);
        if (success) {
            if (button) {
                button.className = 'unfollow-btn';
                button.innerHTML = '<i class="fas fa-user-minus"></i> cancelfollow';
                button.onclick = () => unfollowUserFromProfile(targetUserId);
            }
            showSuccessMessage('Follow Success!', 'Successfully followed this user');
        }
    } catch (error) {
        console.error('Follows fails:', error);
        showSuccessMessage('Follows fails', error.message || 'operation failed');
    } finally {
        const button = document.querySelector('#userProfileFollowButton button');
        if (button) {
            button.disabled = false;
        }
    }
}

// Unfollow user from user profile
async function unfollowUserFromProfile(targetUserId) {
    if (!currentUser.address || !currentUser.id) {
        showSuccessMessage('operation failed', 'Please connect the wallet first');
        return;
    }
    
    try {
        const button = document.querySelector('#userProfileFollowButton .unfollow-btn');
        if (button) {
            button.disabled = true;
            button.innerHTML = '<i class="fas fa-spinner fa-spin"></i> cancelling...';
        }
        
        const success = await unfollowUserById(targetUserId);
        if (success) {
            if (button) {
                button.className = 'follow-btn';
                button.innerHTML = '<i class="fas fa-user-plus"></i> follow';
                button.onclick = () => followUserFromProfile(targetUserId);
            }
            showSuccessMessage('cancelFollow Success!', 'Successfully cancelled following this user');
        }
    } catch (error) {
        console.error('cancelFollows fails:', error);
        showSuccessMessage('cancelFollows fails', error.message || 'operation failed');
    } finally {
        const button = document.querySelector('#userProfileFollowButton button');
        if (button) {
            button.disabled = false;
        }
    }
}

// Load more following users
async function loadMoreFollowing() {
    if (!hasMoreFollowing || isLoadingMoreFollowing) return;
    await loadFollowingList(true);
}

// Load more followers
async function loadMoreFollowers() {
    if (!hasMoreFollowers || isLoadingMoreFollowers) return;
    await loadFollowersList(true);
}

// Load more friends
async function loadMoreFriends() {
    if (!hasMoreFriends || isLoadingMoreFriends) return;
    await loadFriendsList(true);
}

// Daily recommendations related variables
let isLoadingRecommendations = false;
let currentRecommendations = [];

// Load daily recommendations
async function loadDailyRecommendations() {
    if (isLoadingRecommendations) return;
    
    const recommendationsContent = document.getElementById('recommendationsContent');
    if (!recommendationsContent) return;
    
    isLoadingRecommendations = true;
    
    try {
        recommendationsContent.innerHTML = `
            <div class="loading-container">
                <div class="loading-spinner"></div>
                <p>Loading today's hot posts...</p>
            </div>
        `;
        
        const response = await fetch(`${API_BASE}/recommendations/daily?user_address=${encodeURIComponent(walletAccount || '')}`);
        const result = await response.json();
        
        if (result.success && result.data) {
            const { posts, last_refresh_time } = result.data;
            currentRecommendations = posts;
            
            displayRecommendations(posts);
            updateLastRefreshTime(last_refresh_time);
        } else {
            recommendationsContent.innerHTML = `
                <div class="empty-state">
                    <div class="empty-icon">📭</div>
                    <h4>No recommendations</h4>
                    <p>No hot posts today, post your first post now!</p>
                    <button class="create-post-btn" onclick="switchView('posts')">
                        <i class="fas fa-edit"></i> Post now
                    </button>
                </div>
            `;
        }
        
    } catch (error) {
        console.error('Load daily recommendations failed:', error);
        recommendationsContent.innerHTML = `
            <div class="error-state">
                <div class="error-icon">❌</div>
                <h4>Load failed</h4>
                <p>Unable to load recommendations, please try again later</p>
                <button class="retry-btn" onclick="loadDailyRecommendations()">
                    <i class="fas fa-redo"></i> Retry
                </button>
            </div>
        `;
    } finally {
        isLoadingRecommendations = false;
    }
}

// Display recommendations
function displayRecommendations(posts) {
    const recommendationsContent = document.getElementById('recommendationsContent');
    if (!recommendationsContent) return;
    
    if (posts.length === 0) {
        recommendationsContent.innerHTML = `
            <div class="empty-state">
                <div class="empty-icon">📭</div>
                <h4>No recommendations</h4>
                <p>No hot posts today, post your first post now!</p>
                <button class="create-post-btn" onclick="switchView('posts')">
                    <i class="fas fa-edit"></i> Post now
                </button>
            </div>
        `;
        return;
    }
    
    const postsHtml = posts.map((post, index) => {
        const rankIcon = getRankIcon(index + 1);
        const heatScore = post.heat_score ? post.heat_score.toFixed(1) : '0.0';
        const timeAgo = formatTime(post.created_at);
        
        return `
            <div class="recommendation-card" data-post-id="${post.id}" onclick="openPost('${post.id}')">
                <div class="rank-badge">
                    <span class="rank-icon">${rankIcon}</span>
                    <span class="rank-number">#${index + 1}</span>
                </div>
                <div class="post-content">
                    <h4 class="post-title">${escapeHtml(post.title)}</h4>
                    <p class="post-preview">${escapeHtml(post.content.substring(0, 120))}${post.content.length > 120 ? '...' : ''}</p>
                    ${post.image ? `<div class="post-thumbnail"><img src="${post.image}" alt="帖子图片"></div>` : ''}
                    <div class="post-meta">
                        <div class="author-info">
                            <span class="author-name">${escapeHtml(post.author_name || `${post.author_address.slice(0, 6)}...${post.author_address.slice(-4)}`)}</span>
                            <span class="post-time">${timeAgo}</span>
                        </div>
                        <div class="post-stats">
                            <span class="stat-item">
                                <i class="fas fa-fire"></i>
                                <span class="heat-score">${heatScore}</span>
                            </span>
                            <span class="stat-item">
                                <i class="fas fa-heart"></i>
                                <span>${post.likes || 0}</span>
                            </span>
                            <span class="stat-item">
                                <i class="fas fa-comment"></i>
                                <span>${post.comments_count || 0}</span>
                            </span>
                            <span class="stat-item">
                                <i class="fas fa-eye"></i>
                                <span>${post.views || 0}</span>
                            </span>
                        </div>
                    </div>
                    ${post.tags && post.tags.length > 0 ? `
                        <div class="post-tags">
                            ${post.tags.map(tag => `<span class="tag">${escapeHtml(tag)}</span>`).join('')}
                        </div>
                    ` : ''}
                </div>
            </div>
        `;
    }).join('');
    
    recommendationsContent.innerHTML = postsHtml;
}

// Get rank icon
function getRankIcon(rank) {
    switch(rank) {
        case 1: return '🥇';
        case 2: return '🥈';
        case 3: return '🥉';
        case 4: return '🏅';
        case 5: return '🏅';
        default: return '🔥';
    }
}

// Update last refresh time
function updateLastRefreshTime(refreshTime) {
    const lastRefreshTimeElement = document.getElementById('lastRefreshTime');
    if (!lastRefreshTimeElement) return;
    
    if (refreshTime) {
        const refreshDate = new Date(refreshTime);
        const now = new Date();
        const nextRefresh = new Date(refreshDate);
        nextRefresh.setDate(nextRefresh.getDate() + 1);
        nextRefresh.setHours(0, 0, 0, 0);
        
        const timeUntilNext = nextRefresh - now;
        const hoursLeft = Math.floor(timeUntilNext / (1000 * 60 * 60));
        const minutesLeft = Math.floor((timeUntilNext % (1000 * 60 * 60)) / (1000 * 60));
        
        if (timeUntilNext > 0) {
            lastRefreshTimeElement.textContent = `Next refresh in: ${hoursLeft} hours ${minutesLeft} minutes`;
        } else {
            lastRefreshTimeElement.textContent = 'Ready to refresh...';
        }
    } else {
        lastRefreshTimeElement.textContent = '今日首次加载';
    }
}

// Manual refresh recommendations
async function refreshRecommendations() {
    const refreshBtn = document.querySelector('.refresh-btn');
    if (!refreshBtn || isLoadingRecommendations) return;
    
    // Add rotating animation
    refreshBtn.querySelector('i').classList.add('fa-spin');
    refreshBtn.disabled = true;
    
    try {
        await loadDailyRecommendations();
        showSuccessMessage('Refresh success', 'Successfully loaded latest recommendations');
    } catch (error) {
        showWarningToast('Refresh failed, please try again later');
    } finally {
        refreshBtn.querySelector('i').classList.remove('fa-spin');
        refreshBtn.disabled = false;
    }
}

            // Load user avatar and bio
function loadUserAvatarAndBio(userStats) {
    // Set avatar
    const userAvatarImg = document.getElementById('userAvatarImg');
    const userAvatarIcon = document.getElementById('userAvatarIcon');
    const editAvatarBtn = document.getElementById('editAvatarBtn');
    
    if (userStats.avatar) {
        userAvatarImg.src = userStats.avatar;
        userAvatarImg.style.display = 'block';
        userAvatarIcon.style.display = 'none';
        // When there is an avatar, display the change button and hide the hover effect
        if (editAvatarBtn) {
            editAvatarBtn.style.display = 'block';
        }
        const userAvatar = document.querySelector('.user-avatar');
        if (userAvatar) {
            userAvatar.style.cursor = 'default';
            const overlay = userAvatar.querySelector('.avatar-overlay');
            if (overlay) {
                overlay.style.display = 'none';
            }
        }
    } else {
        userAvatarImg.style.display = 'none';
        userAvatarIcon.style.display = 'block';
        // When there is no avatar, hide the change button and keep the hover effect
        if (editAvatarBtn) {
            editAvatarBtn.style.display = 'none';
        }
        const userAvatar = document.querySelector('.user-avatar');
        if (userAvatar) {
            userAvatar.style.cursor = 'pointer';
            const overlay = userAvatar.querySelector('.avatar-overlay');
            if (overlay) {
                overlay.style.display = '';
            }
        }
    }
    
    // Set bio
    const bioText = document.getElementById('bioText');
    const bioTextarea = document.getElementById('bioTextarea');
    const editBioBtn = document.getElementById('editBioBtn');
    const bioDisplay = document.getElementById('bioDisplay');
    
    if (userStats.bio && userStats.bio.trim()) {
        // When there is a bio, display the bio and edit button
        bioText.textContent = userStats.bio;
        editBioBtn.style.display = 'block';
        bioDisplay.style.display = 'block';
    } else {
        // When there is no bio, display the default text
        bioText.textContent = 'No personal bio, click the edit button to add your bio...';
        editBioBtn.style.display = 'block';
        bioDisplay.style.display = 'block';
    }
    
    // Initialize text area
    if (bioTextarea) {
        bioTextarea.value = userStats.bio || '';
        updateBioCharCount();
        
        // Add character count listener (only add once)
        if (!bioTextarea.hasAttribute('data-listener-added')) {
            bioTextarea.addEventListener('input', updateBioCharCount);
            bioTextarea.setAttribute('data-listener-added', 'true');
        }
    }
}

// Update bio character count
function updateBioCharCount() {
    const bioTextarea = document.getElementById('bioTextarea');
    const bioCharCount = document.getElementById('bioCharCount');
    
    if (bioTextarea && bioCharCount) {
        const currentLength = bioTextarea.value.length;
        bioCharCount.textContent = currentLength;
        
        // Change color based on character count
        if (currentLength > 450) {
            bioCharCount.style.color = '#ff4757';
        } else if (currentLength > 400) {
            bioCharCount.style.color = '#ffa502';
        } else {
            bioCharCount.style.color = '#666';
        }
    }
}

// Trigger avatar upload
function triggerAvatarUpload() {
    if (!walletAccount) {
        showWarningToast('Please connect the wallet first');
        return;
    }
    document.getElementById('avatarUpload').click();
}

// Handle avatar upload
async function handleAvatarUpload(event) {
    const file = event.target.files[0];
    if (!file) return;
    
    // Verify file type
    const allowedTypes = ['image/jpeg', 'image/jpg', 'image/png'];
    if (!allowedTypes.includes(file.type)) {
        showWarningToast('Only JPG and PNG images are supported');
        return;
    }
    
  
    const maxSize = 5 * 1024 * 1024;
    if (file.size > maxSize) {
        showWarningToast('Image size cannot exceed 5MB');
        return;
    }
    
    try {
        const formData = new FormData();
        formData.append('avatar', file);
        formData.append('user_address', walletAccount);
        
        showSuccessMessage('Uploading...', 'Uploading avatar...');
        
        const response = await fetch(`${API_BASE}/users/avatar/upload`, {
            method: 'POST',
            body: formData
        });
        
        const result = await response.json();
        
        if (result.success) {
          
            const userAvatarImg = document.getElementById('userAvatarImg');
            const userAvatarIcon = document.getElementById('userAvatarIcon');
            const editAvatarBtn = document.getElementById('editAvatarBtn');
            
            userAvatarImg.src = result.data.avatar_url;
            userAvatarImg.style.display = 'block';
            userAvatarIcon.style.display = 'none';
            
       
            if (editAvatarBtn) {
                editAvatarBtn.style.display = 'block';
            }
            const userAvatar = document.querySelector('.user-avatar');
            if (userAvatar) {
                userAvatar.style.cursor = 'default';
                const overlay = userAvatar.querySelector('.avatar-overlay');
                if (overlay) {
                    overlay.style.display = 'none';
                }
            }
            
            showSuccessMessage('Upload success', 'Avatar updated');
        } else {
            showWarningToast(result.error || 'Avatar upload failed');
        }
    } catch (error) {
        console.error('Avatar upload failed:', error);
        showWarningToast('Avatar upload failed');
    } finally {
        // Clear input field
        event.target.value = '';
    }
}


function toggleBioEdit() {
    const bioDisplay = document.getElementById('bioDisplay');
    const bioForm = document.getElementById('bioForm');
    const editBioBtn = document.getElementById('editBioBtn');
    
    bioDisplay.style.display = 'none';
    bioForm.style.display = 'block';
    editBioBtn.style.display = 'none';
    

    const bioTextarea = document.getElementById('bioTextarea');
    if (bioTextarea) {
        bioTextarea.focus();
    }
}


function cancelBioEdit() {
    const bioDisplay = document.getElementById('bioDisplay');
    const bioForm = document.getElementById('bioForm');
    const editBioBtn = document.getElementById('editBioBtn');
    const bioTextarea = document.getElementById('bioTextarea');
    const bioText = document.getElementById('bioText');
    

    if (bioTextarea && bioText) {
        bioTextarea.value = bioText.textContent === 'No personal bio, click the edit button to add your bio...' ? '' : bioText.textContent;
        updateBioCharCount();
    }
    
    bioDisplay.style.display = 'block';
    bioForm.style.display = 'none';
    editBioBtn.style.display = 'block';
}


async function saveBioProfile() {
    if (!walletAccount) {
        showWarningToast('Please connect the wallet first');
        return;
    }
    
    const bioTextarea = document.getElementById('bioTextarea');
    if (!bioTextarea) return;
    
    const bio = bioTextarea.value.trim();
    
    try {
        const saveBioBtn = document.querySelector('.save-bio-btn');
        const originalText = saveBioBtn.innerHTML;
        saveBioBtn.innerHTML = '<i class="fas fa-spinner fa-spin"></i> 保存中...';
        saveBioBtn.disabled = true;
        
        const response = await fetch(`${API_BASE}/users/bio/update`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            },
            body: JSON.stringify({
                user_address: walletAccount,
                bio: bio
            })
        });
        
        const result = await response.json();
        
        if (result.success) {
       
            const bioText = document.getElementById('bioText');
            const bioDisplay = document.getElementById('bioDisplay');
            const bioForm = document.getElementById('bioForm');
            const editBioBtn = document.getElementById('editBioBtn');
            
            bioText.textContent = bio || 'No personal bio, click the edit button to add your bio...';
            
            // Switch back to display mode
            bioDisplay.style.display = 'block';
            bioForm.style.display = 'none';
            editBioBtn.style.display = 'block';
            
            showSuccessMessage('Save success', 'Personal bio updated');
        } else {
            showWarningToast(result.error || 'Save failed');
        }
    } catch (error) {
        console.error('Save bio failed:', error);
        showWarningToast('Save bio failed');
    } finally {
        const saveBioBtn = document.querySelector('.save-bio-btn');
        saveBioBtn.innerHTML = '<i class="fas fa-save"></i> Save bio';
        saveBioBtn.disabled = false;
    }
}

// ============ Irys Amplifiers ============
let currentAmplifiersWindow = '7d';
let currentAmplifiersData = null;

async function loadAmplifiers(window = '7d') {
    const container = document.getElementById('amplifiersList');
    if (!container) return;

    container.innerHTML = '<div class="loading-spinner">Loading...</div>';

    try {
        const response = await fetch(`${API_BASE}/amplifiers?window=${window}`);
        const data = await response.json();

        if (data.community_mindshare && data.community_mindshare.top_1000_yappers) {
            currentAmplifiersData = data.community_mindshare.top_1000_yappers;
            const top10 = currentAmplifiersData.slice(0, 10);
            renderAmplifiers(top10);
        } else {
            container.innerHTML = '<div class="loading-spinner">No data</div>';
        }
    } catch (error) {
        console.error('Load amplifiers failed:', error);
        container.innerHTML = '<div class="loading-spinner">Load failed</div>';
    }
}

function renderAmplifiers(amplifiers) {
    const container = document.getElementById('amplifiersList');
    if (!container) return;

    container.innerHTML = amplifiers.map((amp, index) => {
        const rank = index + 1;
        const rankClass = rank === 1 ? 'top1' : rank === 2 ? 'top2' : rank === 3 ? 'top3' : '';
        const rankEmoji = rank === 1 ? '🥇' : rank === 2 ? '🥈' : rank === 3 ? '🥉' : rank;

        const impressions = formatNumber(amp.total_impressions);
        const tweets = amp.tweet_counts || 0;
        const engagements = amp.total_likes + amp.total_retweets + amp.total_quote_tweets;

        return `
            <div class="amplifier-item" onclick="window.open('https://twitter.com/${amp.username}', '_blank')">
                <div class="amplifier-rank ${rankClass}">${rankEmoji}</div>
                <div class="amplifier-info">
                    <div class="amplifier-name">${amp.displayname || amp.username}</div>
                    <div class="amplifier-stats">
                        <span class="amplifier-stat">
                            <i class="fas fa-eye"></i> ${impressions}
                        </span>
                        <span class="amplifier-stat">
                            <i class="fas fa-comment"></i> ${tweets}
                        </span>
                    </div>
                </div>
            </div>
        `;
    }).join('');
}

function formatNumber(num) {
    if (num >= 1000000) {
        return (num / 1000000).toFixed(1) + 'M';
    } else if (num >= 1000) {
        return (num / 1000).toFixed(1) + 'K';
    }
    return num.toString();
}

// Search amplifier by username
async function searchAmplifier() {
    const input = document.getElementById('amplifierSearchInput');
    const resultDiv = document.getElementById('searchResult');

    if (!input || !resultDiv) return;

    const username = input.value.trim().toLowerCase();
    if (!username) {
        resultDiv.classList.remove('show');
        return;
    }

    if (!currentAmplifiersData) {
        resultDiv.className = 'search-result show not-found';
        resultDiv.innerHTML = 'Please wait for data to load...';
        return;
    }

    const found = currentAmplifiersData.find(amp =>
        amp.username.toLowerCase() === username ||
        (amp.displayname && amp.displayname.toLowerCase().includes(username))
    );

    if (found) {
        const rank = parseInt(found.rank);
        const impressions = formatNumber(found.total_impressions);
        const tweets = found.tweet_counts || 0;

        resultDiv.className = 'search-result show found';
        resultDiv.innerHTML = `
            <strong>${found.displayname || found.username}</strong><br>
            Rank: #${rank} | ${impressions} views | ${tweets} tweets
        `;
    } else {
        resultDiv.className = 'search-result show not-found';
        resultDiv.innerHTML = `Username "${username}" not found in top 1000`;
    }
}

// Enter key to search
document.addEventListener('DOMContentLoaded', function() {
    const searchInput = document.getElementById('amplifierSearchInput');
    if (searchInput) {
        searchInput.addEventListener('keypress', function(e) {
            if (e.key === 'Enter') {
                searchAmplifier();
            }
        });
    }
});

// Tab switching
document.addEventListener('DOMContentLoaded', function() {
    const tabs = document.querySelectorAll('.amp-tab');
    tabs.forEach(tab => {
        tab.addEventListener('click', function() {
            tabs.forEach(t => t.classList.remove('active'));
            this.classList.add('active');

            const window = this.dataset.window;
            currentAmplifiersWindow = window;
            loadAmplifiers(window);

            // Clear search result
            const resultDiv = document.getElementById('searchResult');
            if (resultDiv) {
                resultDiv.classList.remove('show');
            }
        });
    });

    // Load initial data
    loadAmplifiers('7d');
});