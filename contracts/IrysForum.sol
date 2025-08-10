// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";
import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
import "@openzeppelin/contracts/utils/Counters.sol";

contract IrysForum is Ownable, ReentrancyGuard {
    using Counters for Counters.Counter;
    
   
    Counters.Counter private _postIds;
    Counters.Counter private _commentIds;
    
   
    IERC20 public irysToken;
    
   
    uint256 public postCost = 0.001 ether; 
    uint256 public commentCost = 0.0005 ether; 
    uint256 public usernameCost = 0.002 ether; 
    uint256 public postReward = 100;
    uint256 public commentReward = 50; 
    uint256 public likeReward = 10; 
    uint256 public qualityThreshold = 5; 
    
   
    uint256 public dailyPointsPool = 10000; 
    uint256 public lastMiningResetTime;
    uint256 public currentDayPointsDistributed;
    
   
    struct Post {
        uint256 id;
        address author;
        string title;
        string content;
        string[] tags;
        uint256 timestamp;
        uint256 likes;
        uint256 comments;
        bool qualityPost; 
        string irysTransactionId;
    }
    
    struct Comment {
        uint256 id;
        uint256 postId;
        address author;
        string content;
        uint256 timestamp;
        uint256 likes;
        uint256 parentId; 
        string irysTransactionId;
    }
    
    struct User {
        uint256 postsCount;
        uint256 commentsCount;
        uint256 totalLikesReceived;
        uint256 reputationScore;
        uint256 totalPoints; 
        uint256 totalSpent; 
        bool isMiner; 
        uint256 lastActivityTime;
        string username; 
        bool hasUsername; 
    }
    

    mapping(uint256 => Post) public posts;
    mapping(uint256 => Comment) public comments;
    mapping(address => User) public users;
    mapping(uint256 => mapping(address => bool)) public postLikes;  
    mapping(uint256 => mapping(address => bool)) public commentLikes;  
    mapping(address => uint256) public userDailyContributions;  
    mapping(uint256 => uint256[]) public postComments;  
    mapping(string => bool) public usernameExists;  
    mapping(string => address) public usernameToAddress;  
    
     
    address[] public activeMiners;
    mapping(address => bool) public isMinerActive;
    
 
    event PostCreated(uint256 indexed postId, address indexed author, string title, uint256 pointsEarned);
    event CommentCreated(uint256 indexed commentId, uint256 indexed postId, address indexed author, uint256 pointsEarned);
    event PostLiked(uint256 indexed postId, address indexed liker, address indexed author, uint256 pointsEarned);
    event CommentLiked(uint256 indexed commentId, address indexed liker, address indexed author, uint256 pointsEarned);
    event QualityPostDetected(uint256 indexed postId, address indexed author, uint256 bonusPoints);
    event PointsEarned(address indexed user, uint256 points, string reason);
    event MiningRewardDistributed(address indexed miner, uint256 points);
    event ReputationUpdated(address indexed user, uint256 newReputation);
    event UsernameRegistered(address indexed user, string username);
    
    constructor(address _irysToken) {
        irysToken = IERC20(_irysToken);
        lastMiningResetTime = block.timestamp;
    }
    
   //Register username
    function registerUsername(string memory _username) external payable nonReentrant {
        require(msg.value >= usernameCost, "Insufficient payment for username registration");
        require(bytes(_username).length >= 3 && bytes(_username).length <= 20, "Username must be 3-20 characters");
        require(!usernameExists[_username], "Username already exists");
        require(!users[msg.sender].hasUsername, "User already has a username");
        require(_isValidUsername(_username), "Invalid username format");
        
    
        users[msg.sender].username = _username;
        users[msg.sender].hasUsername = true;
        users[msg.sender].totalSpent += usernameCost;
        usernameExists[_username] = true;
        usernameToAddress[_username] = msg.sender;
        
        emit UsernameRegistered(msg.sender, _username);
    }
    

    function _isValidUsername(string memory _username) internal pure returns (bool) {
        bytes memory usernameBytes = bytes(_username);
        
        for (uint256 i = 0; i < usernameBytes.length; i++) {
            bytes1 char = usernameBytes[i];
            
           
            if ((char >= 0x61 && char <= 0x7A) || (char >= 0x41 && char <= 0x5A)) {
                continue;
            }
         
            if (char >= 0x30 && char <= 0x39) {
                continue;
            }
          
            if (char == 0x5F) {
                continue;
            }
            
          
            return false;
        }
        
        return true;
    }
    
 
    function createPost(
        string memory _title,
        string memory _content,
        string[] memory _tags,
        string memory _irysTransactionId
    ) external payable nonReentrant {
        require(users[msg.sender].hasUsername, "Must register username before posting");
        require(msg.value >= postCost, "Insufficient payment for posting");
        require(bytes(_title).length > 0, "Title cannot be empty");
        require(bytes(_content).length > 0, "Content cannot be empty");
        
        _postIds.increment();
        uint256 newPostId = _postIds.current();
        
      
        posts[newPostId] = Post({
            id: newPostId,
            author: msg.sender,
            title: _title,
            content: _content,
            tags: _tags,
            timestamp: block.timestamp,
            likes: 0,
            comments: 0,
            qualityPost: false,
            irysTransactionId: _irysTransactionId
        });
        
        
        users[msg.sender].postsCount++;
        users[msg.sender].totalSpent += postCost;
        users[msg.sender].lastActivityTime = block.timestamp;
        
   
        uint256 pointsEarned = postReward;
        users[msg.sender].totalPoints += pointsEarned;
        
      
        _updateReputation(msg.sender);
        
       
        _updateMiningContribution(msg.sender, 10); 
        
        emit PostCreated(newPostId, msg.sender, _title, pointsEarned);
        emit PointsEarned(msg.sender, pointsEarned, "Post Creation");
    }
    
    
    function createComment(
        uint256 _postId,
        string memory _content,
        uint256 _parentId,
        string memory _irysTransactionId
    ) external payable nonReentrant {
        require(users[msg.sender].hasUsername, "Must register username before commenting");
        require(msg.value >= commentCost, "Insufficient payment for commenting");
        require(posts[_postId].id != 0, "Post does not exist");
        require(bytes(_content).length > 0, "Content cannot be empty");
        
        _commentIds.increment();
        uint256 newCommentId = _commentIds.current();
        
      
        comments[newCommentId] = Comment({
            id: newCommentId,
            postId: _postId,
            author: msg.sender,
            content: _content,
            timestamp: block.timestamp,
            likes: 0,
            parentId: _parentId,
            irysTransactionId: _irysTransactionId
        });
        
  
        posts[_postId].comments++;
        postComments[_postId].push(newCommentId);
        
   
        users[msg.sender].commentsCount++;
        users[msg.sender].totalSpent += commentCost;
        users[msg.sender].lastActivityTime = block.timestamp;
        
    
        uint256 pointsEarned = commentReward;
        users[msg.sender].totalPoints += pointsEarned;
        
    
        _updateReputation(msg.sender);
        
   
        _updateMiningContribution(msg.sender, 5); 
        
        emit CommentCreated(newCommentId, _postId, msg.sender, pointsEarned);
        emit PointsEarned(msg.sender, pointsEarned, "Comment Creation");
    }
    

    function likePost(uint256 _postId) external nonReentrant {
        require(posts[_postId].id != 0, "Post does not exist");
        require(!postLikes[_postId][msg.sender], "Already liked this post");
        require(posts[_postId].author != msg.sender, "Cannot like own post");
        
        postLikes[_postId][msg.sender] = true;
        posts[_postId].likes++;
        
        address author = posts[_postId].author;
        users[author].totalLikesReceived++;
        
  
        uint256 pointsEarned = likeReward;
        users[author].totalPoints += pointsEarned;
        
     
        if (posts[_postId].likes >= qualityThreshold && !posts[_postId].qualityPost) {
            posts[_postId].qualityPost = true;
            uint256 bonusPoints = postReward * 2; 
            users[author].totalPoints += bonusPoints;
            emit QualityPostDetected(_postId, author, bonusPoints);
            emit PointsEarned(author, bonusPoints, "Quality Post Bonus");
        }
        
      
        _updateReputation(author);
        
   
        _updateMiningContribution(msg.sender, 1); 
        _updateMiningContribution(author, 2); 
        
        emit PostLiked(_postId, msg.sender, author, pointsEarned);
        emit PointsEarned(author, pointsEarned, "Post Liked");
    }
    
  
    function likeComment(uint256 _commentId) external nonReentrant {
        require(comments[_commentId].id != 0, "Comment does not exist");
        require(!commentLikes[_commentId][msg.sender], "Already liked this comment");
        require(comments[_commentId].author != msg.sender, "Cannot like own comment");
        
        commentLikes[_commentId][msg.sender] = true;
        comments[_commentId].likes++;
        
        address author = comments[_commentId].author;
        users[author].totalLikesReceived++;
        
     
        uint256 pointsEarned = likeReward / 2;
        users[author].totalPoints += pointsEarned;
        
       
        _updateReputation(author);
        
        
        _updateMiningContribution(msg.sender, 1);
        _updateMiningContribution(author, 1);
        
        emit CommentLiked(_commentId, msg.sender, author, pointsEarned);
        emit PointsEarned(author, pointsEarned, "Comment Liked");
    }
    
   
    function distributeMiningRewards() external {
        require(block.timestamp >= lastMiningResetTime + 1 days, "Too early to distribute");
        require(currentDayPointsDistributed < dailyPointsPool, "Today's rewards already distributed");
        
        uint256 totalContributions = 0;
        
      
        for (uint256 i = 0; i < activeMiners.length; i++) {
            totalContributions += userDailyContributions[activeMiners[i]];
        }
        
        require(totalContributions > 0, "No contributions to reward");
        
      
        uint256 remainingPoints = dailyPointsPool - currentDayPointsDistributed;
        
        for (uint256 i = 0; i < activeMiners.length; i++) {
            address miner = activeMiners[i];
            uint256 contribution = userDailyContributions[miner];
            
            if (contribution > 0) {
                uint256 pointsReward = (remainingPoints * contribution) / totalContributions;
                if (pointsReward > 0) {
                    users[miner].totalPoints += pointsReward;
                    currentDayPointsDistributed += pointsReward;
                    emit MiningRewardDistributed(miner, pointsReward);
                    emit PointsEarned(miner, pointsReward, "Daily Mining Reward");
                }
            }
        }
        
        
        _resetDailyMining();
    }
    

    function _updateReputation(address user) internal {
        uint256 newReputation = 
            users[user].postsCount * 10 +
            users[user].commentsCount * 5 +
            users[user].totalLikesReceived * 2;
            
        users[user].reputationScore = newReputation;
        emit ReputationUpdated(user, newReputation);
    }
    
  
    function _updateMiningContribution(address user, uint256 points) internal {
        userDailyContributions[user] += points;
        
        
        if (!isMinerActive[user]) {
            activeMiners.push(user);
            isMinerActive[user] = true;
            users[user].isMiner = true;
        }
    }
    
    
    function _resetDailyMining() internal {
        lastMiningResetTime = block.timestamp;
        currentDayPointsDistributed = 0;
        
     
        for (uint256 i = 0; i < activeMiners.length; i++) {
            userDailyContributions[activeMiners[i]] = 0;
            isMinerActive[activeMiners[i]] = false;
        }
        delete activeMiners;
    }
    
   
    function getPost(uint256 _postId) external view returns (Post memory) {
        return posts[_postId];
    }
    

    function getComment(uint256 _commentId) external view returns (Comment memory) {
        return comments[_commentId];
    }
    

    function getUser(address _user) external view returns (User memory) {
        return users[_user];
    }
    
 
    function getPostComments(uint256 _postId) external view returns (uint256[] memory) {
        return postComments[_postId];
    }
    

    function isUsernameAvailable(string memory _username) external view returns (bool) {
        return !usernameExists[_username] && _isValidUsername(_username);
    }
    

    function getUsernameByAddress(address _user) external view returns (string memory) {
        return users[_user].username;
    }
    
 
    function getAddressByUsername(string memory _username) external view returns (address) {
        return usernameToAddress[_username];
    }
    
  
    function updateEconomicParameters(
        uint256 _postCost,
        uint256 _commentCost,
        uint256 _postReward,
        uint256 _commentReward,
        uint256 _likeReward
    ) external onlyOwner {
        postCost = _postCost;
        commentCost = _commentCost;
        postReward = _postReward;
        commentReward = _commentReward;
        likeReward = _likeReward;
    }
    
  
    function updateDailyPointsPool(uint256 _newPool) external onlyOwner {
        dailyPointsPool = _newPool;
    }
    

    function emergencyWithdraw(uint256 amount) external onlyOwner {
        require(amount <= address(this).balance, "Insufficient balance");
        payable(owner()).transfer(amount);
    }
    

    function getContractBalance() external view returns (uint256) {
        return address(this).balance;
    }
    
 
    receive() external payable {
        
    }
} 