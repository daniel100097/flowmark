Rails.application.routes.draw do
  resources :teams, only: [] do
    resources :invitations, only: [:create]
  end
  post "/invitations/:token/accept", to: "invitation_acceptances#create", as: :accept_invitation
end
